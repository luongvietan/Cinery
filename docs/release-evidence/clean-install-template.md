# Clean-Install Smoke Test Record

Copy this template to `docs/release-evidence/<date>-clean-install.md` and
fill it in during an actual clean-machine (or clean OS user profile) run.
A clean-install pass is the ONLY gate that promotes documentation from
`MVP RELEASE CANDIDATE` to `MVP IMPLEMENTED`.

## Environment

- Tester name:
- Date/time (start / end):
- Machine type (clean machine / clean OS user profile / VM):
- OS name and build:
- Source checkout, development database, build tools, and prior app data
  unavailable to the test user: [ ] yes [ ] no

## Installer

- Installer absolute path:
- Installer SHA-256:
- Installer size (bytes):
- App version from the bundle:
- Install result: [ ] pass [ ] fail — notes:
- Security prompts observed (SmartScreen/UAC etc.):
- First launch from installed shortcut: [ ] pass [ ] fail

## Smoke flow (deterministic, mock provider)

- [ ] Create/open project through the UI
- [ ] Configure provider credential via the OS credential vault
- [ ] Complete Canon setup (story, character, locked sections)
- [ ] Run the deterministic mock Face workflow; inspect its result set
- [ ] Generate and promote Outfit and Character Sheet
- [ ] Assemble World / Scene / Shot / Prop / Keyframe in Cinema
- [ ] Compile and export the cinema prompt
- [ ] Inspect provenance back to workflows and providers
- [ ] Quit and relaunch; verify persistence of exact references and statuses
- [ ] Export diagnostics; bundle contains no media or secrets

## Uninstall and residue

- [ ] Uninstall succeeded
- [ ] User-selected project directories remain intact
- [ ] Residual application data observed (list paths if any)

## Evidence attachments

- Screenshots / log paths:
- Deviations from the checklist:

## Result

- Overall: [ ] PASS [ ] FAIL
- Promote release status to `MVP IMPLEMENTED`: [ ] yes [ ] no
