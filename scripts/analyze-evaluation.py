"""Private raw personal-comparison tables. No provider, network, or automatic efficacy judgment."""
from pathlib import Path
import csv, json, math, statistics, sys
from datetime import datetime


def dt(value):
    return datetime.fromisoformat(value.replace('Z', '+00:00')) if value else None


def safe_cell(value):
    # Preserve numbers while making free-text CSV cells inert in a spreadsheet.
    if isinstance(value, str) and value.lstrip().startswith(('=', '+', '-', '@')):
        return "'" + value
    return value


def analyze(data):
    if data.get('protocol') != 'personal-comparison-v1' or type(data.get('simulated')) is not bool:
        raise ValueError('Use the complete versioned comparison export, not a study archive or partial snapshot.')
    topic_rows, pair_rows, differences = [], [], []
    for pair in data.get('pairs', []):
        native_sim = pair.get('simulated')
        if type(native_sim) is not bool:
            raise ValueError('Every pair must preserve its simulation flag.')
        by_condition = {}
        for topic in pair.get('topics', []):
            response, lesson = topic.get('responses') or {}, topic.get('lesson') or {}
            scores = (topic.get('scores') or [{}])[-1]
            maximum = sum(i['max_points'] for i in topic.get('items', []))
            points = sum(float(s['points']) for s in scores.get('scores', [])) if scores else None
            if points is not None and (not math.isfinite(points) or not maximum or not 0 <= points <= maximum):
                raise ValueError('Invalid manual score scale.')
            events = topic.get('events', [])
            automatic = topic.get('evidence', {}).get('usage', [])
            start, end = dt(lesson.get('completed_at')), dt(response.get('submitted_at'))
            from_window = dt(lesson.get('review_exclusion_at'))
            outside_window = sum(e.get('kind') == 'outside_exposure' and from_window is not None
                                 and dt(e.get('recorded_at')) is not None
                                 and dt(e['recorded_at']) >= from_window for e in events)
            reported_help = response.get('outside_help', False)
            automatic_exposure = topic.get('recorded_exposures_in_exclusion_window', 0)
            flags = []
            if native_sim or data['simulated'] or topic.get('simulated'):
                flags.append('simulated: excluded from actual paired outcomes')
            if not response:
                flags.append('assessment missing')
            if points is None:
                flags.append('separate manual score missing')
            if reported_help:
                flags.append('help reported during assessment')
            if automatic_exposure or outside_window:
                flags.append('exposure reported/recorded in review exclusion window')
            if any('pending' in i.get('review_provenance', '').lower() for i in topic.get('items', [])):
                flags.append('subject review pending')
            if any(u.get('cost_status') == 'estimated' for u in automatic):
                flags.append('uncertain provider cost retained')
            row = dict(pair_id=pair['id'], topic_id=topic['id'], topic=topic['name'],
                       condition=topic['condition'], simulated=bool(native_sim or data['simulated'] or topic.get('simulated')),
                       prior_knowledge=topic['prior_knowledge'], initial_lesson_completed=lesson.get('completed_at', ''),
                       assessment_due=lesson.get('assessment_due_at', ''), submitted=response.get('submitted_at', ''),
                       actual_delay_hours=(end-start).total_seconds()/3600 if end and start else '',
                       score_points=points if points is not None else '', maximum_points=maximum,
                       score_percent=points/maximum*100 if points is not None and maximum else '',
                       familiar_points=next((s['points'] for s in scores.get('scores', [])
                           if any(i['id'] == s['item_id'] and i['form'] == 'familiar' for i in topic.get('items', []))), ''),
                       changed_context_points=next((s['points'] for s in scores.get('scores', [])
                           if any(i['id'] == s['item_id'] and i['form'] == 'changed' for i in topic.get('items', []))), ''),
                       reviewer=scores.get('reviewer', ''), blinded=scores.get('blinded', ''),
                       confidence_by_item=json.dumps({a['item_id']:a['confidence'] for a in response.get('responses', [])}, sort_keys=True),
                       cost_scope='Recorded app charges and manually reported costs; unreported external use remains unknown',
                       review_provenance='user-entered, not independently verified by software',
                       assessment_help_reported=reported_help, exclusion_linked_help=automatic_exposure,
                       exclusion_reported_exposures=outside_window,
                       tracked_seconds=sum(e.get('seconds', 0) for e in events if 'started_at' in e),
                       manual_seconds=sum(e.get('seconds', 0) for e in events if 'recorded_at' in e),
                       heldout_seconds=response.get('elapsed_seconds', ''),
                       recorded_cost_usd=(sum(u['cost_microusd'] for u in automatic)+sum(e.get('cost_microusd', 0) for e in events))/1e6,
                       uncertain_charge_count=sum(u.get('cost_status') == 'estimated' for u in automatic),
                       time_allowance_minutes=pair['registration']['time_allowance_minutes'], flags='; '.join(flags),
                       versions=json.dumps(pair.get('versions', {}), sort_keys=True))
            topic_rows.append(row)
            by_condition[row['condition']] = row
        tool, baseline = by_condition.get('Study Harness'), by_condition.get('baseline')
        complete = tool and baseline and not tool['simulated'] and not baseline['simulated'] and all(r['score_percent'] != '' for r in [tool, baseline])
        clean = complete and all(not r['assessment_help_reported'] and not r['exclusion_linked_help'] and not r['exclusion_reported_exposures'] for r in [tool, baseline])
        diff = tool['score_percent']-baseline['score_percent'] if complete else ''
        pair_rows.append(dict(pair_id=pair['id'], actual_scored_pair=bool(complete),
                              reported_unaided_without_recorded_window_exposure=bool(clean),
                              tool_minus_baseline_percentage_points=diff,
                              tool_recorded_total_seconds=sum(tool[k] or 0 for k in ['tracked_seconds', 'manual_seconds', 'heldout_seconds']) if tool else '',
                              baseline_recorded_total_seconds=sum(baseline[k] or 0 for k in ['tracked_seconds', 'manual_seconds', 'heldout_seconds']) if baseline else '',
                              decision_criterion=pair['registration']['decision_criterion'],
                              comparability='author judgment; not independent proof',
                              status='raw paired observations only; inspect confounds, assistance and reviewer provenance'))
        if clean:
            differences.append(diff)
    summary = dict(actual_scored_pairs=sum(r['actual_scored_pair'] for r in pair_rows),
                   reported_unaided_pairs_without_recorded_window_exposure=len(differences),
                   mean_raw_difference_percentage_points=statistics.mean(differences) if differences else None,
                   descriptive_difference_range=[min(differences), max(differences)] if differences else None,
                   sample_standard_deviation=statistics.stdev(differences) if len(differences) >= 2 else None,
                   interpretation='Descriptive paired data, not a population-effect estimate. Topic comparability and absence of outside help remain unverified; review missingness, time, costs and confounds. A small or inconclusive comparison remains inconclusive.' if differences else 'No actual unaided paired learning outcomes are available. Simulated rows are excluded; no learning-effect claim is produced.')
    return topic_rows, pair_rows, summary


def write_tables(data, directory):
    topics, pairs, summary = analyze(data)
    directory.mkdir(parents=True, exist_ok=True)
    for name, rows in [('topics', topics), ('pairs', pairs)]:
        with (directory/f'{name}.csv').open('w', newline='') as file:
            writer = csv.DictWriter(file, fieldnames=list(rows[0]) if rows else ['status'])
            writer.writeheader()
            for row in rows:
                writer.writerow({k: safe_cell(v) for k, v in row.items()})
    (directory/'descriptive-summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    return summary


if __name__ == '__main__':
    if len(sys.argv) != 3:
        raise SystemExit('Usage: python3 scripts/analyze-evaluation.py comparison.json output-directory')
    source = Path(sys.argv[1])
    if source.stat().st_size > 128*1024*1024:
        raise SystemExit('Comparison export exceeds 128 MiB.')
    summary = write_tables(json.loads(source.read_text()), Path(sys.argv[2]))
    print(json.dumps(summary, indent=2))
