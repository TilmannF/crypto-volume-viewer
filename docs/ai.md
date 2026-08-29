# How this project is written

Crypto Volume Viewer is **written by AI**, directed by a human (Tilmann Felgner). There is no hidden human-authored core behind the model output. The human sets product rules, reviews, runs the machine, and decides what ships.

That is a fact about this repository, not a quality claim. Tests, review, and your own reading are how to judge the code.

## Models used

| Period | Tool |
|---|---|
| Start | OpenAI Codex |
| Middle | Claude |
| Recent weeks | Grok (xAI) |

Normative rules the models are required to follow live in `AGENTS.md` and `policies/`. Those files are part of the public repo on purpose.

## What this means for contributors

A pull request can be typed by a human or an AI. Same bar either way: small change, tests, no secrets, no cracking features, no silent public-behavior change. See [CONTRIBUTING.md](../CONTRIBUTING.md).
