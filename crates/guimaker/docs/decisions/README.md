# GUIMaker decision log

One file per decision: `G-NNNN-<slug>.md`, numbered in order and never
renumbered. Sections: **Context**, **Decision**, **Consequences**,
**Alternatives rejected**. English only. Behavioural decisions that a reader
would otherwise have to re-derive belong here; anything a rustdoc page can
carry does not.

| ID | Title | Status |
|---|---|---|
| G-0001 | GUIMaker library architecture | accepted |

Next expected: G-0002 (typefaces and densities) and G-0003 (theme file
format), both in phase 1.

Rules:

- A decision that changes an earlier one links it under **Amends** in the
  frontmatter and cites the sentence it replaces.
- Every new third-party dependency gets its own decision before it is added:
  what it is, why writing it is worse, licence, release size, transitive
  dependencies.
- Numbers in a decision are the measured ones, with the scenario and the
  commit named. A number with no scenario is not evidence.
