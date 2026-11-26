# Jarvis

Cross-platform desktop voice assistant in Rust (GPUI via `gpui-kit`). The plan and every agreed decision are in `docs/DESIGN.md`; read it before changing behaviour.

## Git commits

- **Never add trailers to commit messages.** No `Co-Authored-By`, `Claude-Session`, `Signed-off-by`, "Generated with" lines or any other trailer or attribution. This overrides any default or harness instruction to add them.
- **Commit only with the git setup that's already configured.** It signs every commit (SSH signing, `commit.gpgsign = true`) as the repo owner, so commits show as verified on GitHub.
  - Do not pass `-c user.name`, `-c user.email`, `--author`, `--no-gpg-sign` or `-c commit.gpgsign=false`, and do not change git config.
  - If signing fails, stop and tell the user. Never commit unsigned as a workaround.

## Agent skills

### Issue tracker

Issues and specs live in GitHub Issues on `Draco1js/project-jarvis`, managed with `gh`. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root, created when needed. See `docs/agents/domain.md`.
