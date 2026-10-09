# Your first folder in PhotoFindry

Start with a small folder of photographs you can safely test. Your originals remain in place; PhotoFindry builds a local library of saved details and previews.

## Set up a model

Use Getting Started or **PhotoFindry → Model Setup…**. Select a compatible vision model's GGUF weights and matching multimodal projector. Files can live in any folder you choose; there is no required Models directory. Follow the app's supported model guidance rather than assuming any GGUF will work.

The app includes its local runtime, search models and face models. You do not need to install a separate AI server. An optional search reranker can be selected or explicitly downloaded separately. Model licences still apply.

“Model stopped” is not lost setup: a saved compatible model starts automatically when tagging needs it. Do not repeat setup solely because the model is unloaded.

## Add and tag photographs

1. Choose **File → Add Photo Folder…** or the toolbar folder button.
2. Select a small folder and allow the access macOS asks you to grant.
3. Browse filenames immediately. Choose Tag library or a scoped tagging action when you want local descriptions.
4. Watch Jobs & Activity for progress. Pause retains the job and its saved progress. Resume continues it without requiring source reconnection merely because it was paused. Stop cancels remaining work and keeps accepted saved results.
5. Review an individual photograph's details and correct names when needed. AI observations and uncertain matches can be wrong.

Finding new files adds records; it does not rewrite or delete the original photographs. Large archives take time. Keep the app open and Mac awake for scheduled processing; schedules do not wake a sleeping Mac.

## Browse naturally

Single click selects a photograph. Command-click toggles selection; Shift-click extends a range. Double click or Space opens a preview. Selection actions apply to the selected records; source and folder actions have separate scope.

People lets you name confirmed reference faces and review uncertain matches. Confident qualified matches can inherit a user-confirmed name. Correct mistakes; a model appearance estimate is not a verified identity.

## Photo detail review and model activity

Settings → Photo detail review keeps review choices separate from the tagging model. The current model remains available. Optional Intern-Decision 4B and experimental CLEF Flash reviewers use separately selected local weights; downloads require an explicit action. CLEF Flash currently requires at least 24 GiB RAM. Intern normally requires 16 GiB and offers an explicit low-memory trial for 8 GiB Macs; broader 8 GiB hardware qualification remains pending.

Experimental review answers remain suggestions. They do not automatically change accepted tags, people names or search evidence. The inspector shows the saved claim behind a suggestion and bounded review coverage. Review again can check eligible unanswered details. Recently improved uses the saved review time.

The improvement strip shows the current prepared photograph, actual progress and the last committed review. The model sidebar follows the actual loaded model and task. Activity totals retain reported token usage across model restarts, with unavailable or incomplete measurements identified. Unload pauses that model's task and preserves pending work for Resume.

## Find help again

The Help menu offers Getting Started and Privacy & Distribution. [Search](https://photofindry.com/docs/search.html), [sources and backups](https://photofindry.com/docs/sources-and-backups.html), and [troubleshooting](https://photofindry.com/docs/troubleshooting.html) explain the next steps.
