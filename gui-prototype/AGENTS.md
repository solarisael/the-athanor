# GUI Prototype Task Rule

Before you inspect, edit, or verify any file in `gui-prototype/`, MUST read `gui-prototype/LESSONS_MAP.md`. It is the rules file for this folder. The repository-root lesson map does not replace it. Read `NAVIGATION_MAP.md` for the routes and the code that owns them.

`gui-desktop/` is the production server. Its `pulse.exe` embeds the files of `gui-prototype/` (`gui-desktop/src/proxy.rs:6-12`). A change here therefore changes what `pulse.exe` serves after its next build. There is no `gui/` folder.

If the task proposes to move an accepted behavior into production, follow the [Promotion gate](LESSONS_MAP.md#promotion-gate) in `LESSONS_MAP.md`.
