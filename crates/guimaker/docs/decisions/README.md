# GUIMaker decision log

One file per decision: `G-NNNN-<slug>.md`, numbered in order and never
renumbered. Sections: **Context**, **Decision**, **Consequences**,
**Alternatives rejected**. English only. Behavioural decisions that a reader
would otherwise have to re-derive belong here; anything a rustdoc page can
carry does not.

| ID | Title | Status |
|---|---|---|
| G-0001 | GUIMaker library architecture | accepted |
| G-0002 | Typefaces and densities | accepted |
| G-0003 | Theme file format | accepted |
| G-0004 | Grapheme segmentation for the text editor | accepted |
| G-0010 | Popup present backend (swapchain vs layered) | accepted |
| G-0011 | Main-window backend default (Vulkan vs DX12) | accepted |
| G-0012 | Flat2D Windows backend and DX12 compiler | accepted |

Next expected: G-0013.

Rules:

- A decision that changes an earlier one links it under **Amends** in the
  frontmatter and cites the sentence it replaces.
- Every new third-party dependency gets its own decision before it is added:
  what it is, why writing it is worse, licence, release size, transitive
  dependencies.
- Numbers in a decision are the measured ones, with the scenario and the
  commit named. A number with no scenario is not evidence.
