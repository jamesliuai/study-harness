# Attribution

Study Harness is directly inspired by [LearnHarness](https://github.com/amosblomqvist/learnharness) by Eero Alvar, especially its emphasis on learner participation, spoken or written reasoning, targeted teaching, and practice.

Study Harness is independently implemented. No LearnHarness application code, prompts, icons, or assets have been copied. Its original application code, tests, prompts, and interface copy are released under the [MIT License](../LICENSE).

Interactive visualizations come from [Explorable Models](https://github.com/jamesliuai/Explorable-Models), a separate MIT-licensed project. Study Harness consumes its exact packed dependencies and extracts the SDK's native validator, canonicalization code, and schemas unchanged. The consumed source commit and package hashes are recorded in [the manifest](../vendor/visualizer/manifest.json).

Third-party dependencies retain their own licenses. Original available license texts and notices are preserved in [THIRD-PARTY-NOTICES.txt](../licenses/THIRD-PARTY-NOTICES.txt), with a [machine-readable inventory](../licenses/inventory.json). This includes the bundled Inter, KaTeX, and PDF fonts. The application also uses system fonts.
