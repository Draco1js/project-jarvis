# Jarvis

Cross-platform desktop voice assistant in Rust (GPUI via `gpui-kit`). The plan and every agreed decision are in `docs/DESIGN.md`; read it before changing behaviour.

## Agent skills

### Issue tracker

Issues and specs live in GitHub Issues on `Draco1js/project-jarvis`, managed with `gh`. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root, created when needed. See `docs/agents/domain.md`.
