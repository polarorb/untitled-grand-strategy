---
name: devblog
description: Add or update entries in the development blog (docs/devlog.md) on the GitHub Pages site. Use at the end of any substantial work session, and whenever the user asks to update the site.
---

# Development blog

The devlog lives at `docs/devlog.md`, served on the public Pages site
(https://polarorb.github.io/untitled-grand-strategy/devlog). Newest
entries at the top. It is a DEVELOPMENT diary, not marketing: written for
people who want to know how the game is actually being built.

## Entry format

```markdown
## YYYY-MM-DD — Short title

One-paragraph summary of what landed.

- The interesting technical decisions and why (link design docs/research
  notes rather than re-explaining them).
- Honest notes: what went wrong, what was cut, what's still janky.
- Numbers where they tell a story (test counts, perf, data volumes).

*Optional: one screenshot from docs/media/ if something visible changed.*
```

## Rules

- One entry per substantial session/milestone — batch small fixes into
  the next real entry rather than posting noise.
- Write in first person plural or neutral ("we", "the sim now...").
  Credit tools honestly — this project is built with Claude Code and an
  agent-swarm research workflow; that's part of the story, don't hide it.
- Keep entries under ~300 words; link to design docs, research readups,
  and commits for depth.
- When a player-visible feature lands, also refresh `docs/index.md`
  (feature list, screenshots) in the same commit.
- The site deploys on push to main — finish by pushing and confirming
  the Pages build isn't broken (curl the URL if in doubt).
