---
"ody-code": patch
---

Make `/model` available inside office-hours and game-design modes. These modes
were locked down to `/exit` only, but they can now be pinned to their own model
(`modeModels.officeHours` / `modeModels.gameDesign`), so the user must be able to
switch models from inside the mode. `/model` is now visible in the command menu
and runnable in every mode; the rest of the special-mode lockdown is unchanged.
