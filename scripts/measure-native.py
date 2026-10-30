"""Read macOS process counters for explicitly selected application PIDs.

No process memory, credentials, or study content is read. Use the SDK's public
proc_pid_rusage v4 ABI. CPU counters are Mach time (not nanoseconds): see Apple's
XNU fill_task_rusage and task_power_info_locked; convert with mach_timebase_info.
"""
import argparse
import ctypes as C
import datetime
import json
import platform
import subprocess
import time
from pathlib import Path


class Usage(C.Structure):
    _fields_ = [("uuid", C.c_uint8 * 16)] + [
        (name, C.c_uint64) for name in (
            "user system idle_wakeups interrupt_wakeups pageins wired resident footprint "
            "start exit child_user child_system child_idle child_interrupt child_pageins "
            "child_elapsed disk_read disk_written qos_default qos_maintenance qos_background "
            "qos_utility qos_legacy qos_initiated qos_interactive billed_system serviced_system "
            "logical_writes lifetime_max_footprint instructions cycles billed_energy "
            "serviced_energy interval_max_footprint runnable"
        ).split()
    ]


class Timebase(C.Structure):
    _fields_ = [("numer", C.c_uint32), ("denom", C.c_uint32)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pids", type=int, nargs="+", required=True)
    parser.add_argument("--seconds", type=float, default=10)
    parser.add_argument("--label", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != "Darwin" or not 1 <= args.seconds <= 60:
        parser.error("Requires macOS and a duration between 1 and 60 seconds")
    library = C.CDLL("/usr/lib/libproc.dylib", use_errno=True)
    library.proc_pid_rusage.argtypes = [C.c_int, C.c_int, C.c_void_p]
    library.proc_pid_rusage.restype = C.c_int
    system = C.CDLL("/usr/lib/libSystem.B.dylib")
    base = Timebase()
    if system.mach_timebase_info(C.byref(base)) or not base.denom:
        raise RuntimeError("Cannot establish Mach clock conversion")

    def read(pid):
        value = Usage()
        if library.proc_pid_rusage(pid, 4, C.byref(value)):
            raise OSError(C.get_errno(), f"Cannot read counters for PID {pid}")
        return {
            "cpu_seconds": (value.user + value.system) * base.numer / base.denom / 1e9,
            "resident_bytes": value.resident,
            "physical_footprint_bytes": value.footprint,
            "process_start_mach": value.start,
        }

    # Verify the clock conversion against Python's per-process CPU clock here.
    import os
    own_before = read(os.getpid())["cpu_seconds"]
    python_before = time.process_time()
    until = time.perf_counter() + 0.12
    while time.perf_counter() < until:
        pass
    python_delta = time.process_time() - python_before
    own_delta = read(os.getpid())["cpu_seconds"] - own_before
    ratio = own_delta / python_delta
    if not 0.85 <= ratio <= 1.15:
        raise RuntimeError(f"CPU clock calibration failed: {ratio}")

    names = {
        str(pid): subprocess.check_output(
            ["ps", "-p", str(pid), "-o", "comm="], text=True
        ).strip() for pid in args.pids
    }
    started = time.perf_counter()
    samples = []
    while True:
        elapsed = time.perf_counter() - started
        samples.append({"elapsed_seconds": elapsed,
                        "processes": {str(pid): read(pid) for pid in args.pids}})
        if elapsed >= args.seconds:
            break
        time.sleep(min(0.25, args.seconds - elapsed))
    first, last = samples[0], samples[-1]
    elapsed = last["elapsed_seconds"] - first["elapsed_seconds"]
    for pid in names:
        if first["processes"][pid]["process_start_mach"] != last["processes"][pid]["process_start_mach"]:
            raise RuntimeError("PID reused during measurement")
    cpu = sum(last["processes"][pid]["cpu_seconds"] - first["processes"][pid]["cpu_seconds"] for pid in names)
    result = {
        "kind": "Native macOS explicit-PID differential process counters",
        "captured_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "label": args.label, "processes": names,
        "mach_timebase": {"numer": base.numer, "denom": base.denom},
        "clock_calibration_ratio": ratio,
        "duration_seconds": elapsed, "cpu_percent_of_one_core": cpu / elapsed * 100,
        "peak_sum_resident_bytes": max(sum(p["resident_bytes"] for p in s["processes"].values()) for s in samples),
        "peak_sum_physical_footprint_bytes": max(sum(p["physical_footprint_bytes"] for p in s["processes"].values()) for s in samples),
        "samples": samples,
        "limitations": "Only named PIDs, sampled memory, no GPU frame-time or energy measurement; shared-process attribution requires a separately recorded before/after inventory.",
        "cpu_units_source": "https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/kern/task.c#L6391",
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({k: v for k, v in result.items() if k != "samples"}, indent=2))


if __name__ == "__main__":
    main()
