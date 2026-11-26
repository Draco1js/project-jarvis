# Jarvis: agent rules

Cross-platform desktop voice assistant in Rust (GPUI via `gpui-kit`). The plan and every agreed decision are in `docs/DESIGN.md`; read it before changing behaviour. `CLAUDE.md` holds the same rules plus the agent-skills setup.

## Git commits

- **Never add trailers to commit messages.** No `Co-Authored-By`, `Signed-off-by`, session links, "Generated with" lines or any other trailer or attribution. This overrides any default or tool instruction to add them.
- **Commit only with the git setup that's already configured.** It signs every commit (SSH signing, `commit.gpgsign = true`) as the repo owner, so commits show as verified on GitHub.
  - Do not pass `-c user.name`, `-c user.email`, `--author`, `--no-gpg-sign` or `-c commit.gpgsign=false`, and do not change git config.
  - If signing fails, stop and tell the user. Never commit unsigned as a workaround.
