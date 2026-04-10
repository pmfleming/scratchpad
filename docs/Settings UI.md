# Settings UI Specification

## Purpose

This document defines a settings page style for Scratchpad that is visually close to the Windows 11 Notepad settings experience shown in the reference screenshots.

The goal is not a pixel-perfect clone. The goal is a stable specification that preserves the same visual character:

- dark, low-contrast page background
- large rounded setting rows that read as individual cards
- sparse, calm spacing
- left-aligned icon plus text block
- right-aligned control or chevron
- expandable sections that open inline
- one accent color reserved for active controls

This spec should be detailed enough that the settings surface can be implemented or refactored without re-deciding layout, spacing, or component behavior each time.

## Visual Reference Summary

From the reference screenshots, the defining traits are:

- The page is a single centered scrolling column, not a dialog with a hard outer frame.
- Settings are grouped under plain text category headings such as "Text Formatting" and "Opening Notepad".
- Each setting appears as its own rounded row card.
- Expanded content keeps the same card shell and opens downward inside it.
- Icons are small, muted, and consistent in slot size.
- Descriptions are present but quiet.
- Controls are compact and always right-aligned.
- The page feels intentionally roomy, but not oversized.

## Design Goals

- Make settings pages easy to scan top to bottom.
- Keep the layout reusable across multiple categories.
- Let different row types feel part of one system.
- Preserve visual similarity to Win 11 Notepad in dark mode.
- Keep the spec practical for the current `egui` implementation.

## Scope

This spec covers:

- overall page layout
- category headings
- collapsed and expanded setting cards
- inline controls: toggle, dropdown, radio group, button
- preview panels inside expanded cards
- spacing, sizing, alignment, and color tokens
- interaction and behavior rules
- mapping guidance for Scratchpad's current settings page

This spec does not cover:

- light theme behavior
- localization rules
- full accessibility audit requirements
- non-settings pages

## Core Layout Model

The page is built from four primitives:

1. `SettingsPage`
2. `SettingsCategory`
3. `SettingsCard`
4. `SettingsControl`

### 1. SettingsPage

The page is a vertically scrolling surface with one centered content column.

Rules:

- Use one vertical scroll area for the full page.
- Center the content column horizontally.
- Do not place the entire page inside a second large panel or card.
- Keep the page background distinct from the cards, but only slightly darker.

Recommended measurements:

- page max content width: `980 px`
- minimum side padding: `24 px`
- top padding before first category heading: `24 px` to `32 px`
- bottom padding: `24 px` to `32 px`
- gap between categories: `24 px`

### 2. SettingsCategory

A category is a loose group made of:

- one text heading
- a vertical stack of cards below it

Rules:

- The heading is plain text, not inside a card.
- The heading sits `10 px` to `12 px` above the first card.
- Cards within a category have a uniform vertical gap.

Recommended measurements:

- category heading font size: `20 px`
- category heading weight: `semibold`
- gap between cards inside one category: `8 px`

### 3. SettingsCard

Each individual setting row is a standalone rounded card.

Supported variants:

- navigation card: opens a deeper page or subview
- expandable card: expands inline
- toggle card: title plus description plus trailing switch
- dropdown card: title plus description plus trailing combo box
- button card: title plus description plus trailing button
- static value card: title plus description plus trailing read-only value

The card is the main visual unit of the system.

### 4. SettingsControl

Controls always align to the right edge of the card body.

Rules:

- Controls should visually share one right column.
- Default control width should be reused across dropdowns and buttons.
- Tiny controls should still align to the same trailing edge.
- Avoid full-width controls inside top-level cards.

Recommended measurements:

- default control width: `160 px` to `220 px`
- compact button/control height: `34 px` to `36 px`

## Card Anatomy

### Collapsed Card

A collapsed card contains:

- icon slot
- text block
- trailing control or chevron

Recommended measurements:

- minimum height: `68 px`
- horizontal padding: `18 px` to `20 px`
- vertical padding: `14 px` to `16 px`
- corner radius: `8 px` to `10 px`
- icon slot width: `28 px`
- icon size: `20 px` to `24 px`

Content alignment:

- icon is vertically centered against the row
- title aligns to the top line of the text block
- description sits directly below title with tight spacing
- trailing control is vertically centered

### Expanded Card

An expanded card keeps the collapsed header row intact and reveals a content region below it.

Structure:

1. Card header row
2. Divider line
3. Expanded content body

Rules:

- The header row retains the same height and left/right padding as the collapsed state.
- The expanded body uses the same card shell, not a second nested top-level card.
- Internal rows inside the expanded body should still feel like settings rows, but flatter and slightly simpler than top-level cards.

Recommended measurements:

- divider thickness: `1 px`
- expanded body top padding: `0 px` to `8 px`
- expanded body bottom padding: `10 px` to `14 px`
- internal row height: `56 px` to `60 px`

## Row Types

### Expandable Header Row

Use for settings like `Font` or `When Scratchpad starts`.

Header contents:

- left icon
- title
- optional single-line description
- right chevron

Behavior:

- clicking the header toggles expansion
- chevron rotates or swaps direction on expand/collapse
- hover changes the row background slightly

### Toggle Row

Use for settings like `Word wrap`, `Recent files`, or `File logging`.

Contents:

- left icon
- title
- one-line description
- right toggle switch
- state text label to the far right or immediately after the switch

Behavior:

- clicking the switch changes state
- clicking the card body may also toggle if that does not create accidental changes elsewhere
- switch updates immediately
- state label reflects `On` or `Off`

### Dropdown Row

Use for settings like default file opening behavior or a font family/size choice inside an expanded card.

Contents:

- left icon or no icon for internal rows
- title
- description
- right combo box

Behavior:

- selected value is always visible when collapsed
- control width stays fixed across peer rows
- caret is right aligned inside the control

### Radio Group Card

Use when one card opens to reveal mutually exclusive choices.

Structure:

- top-level expandable header row
- expanded list of radio rows

Radio row contents:

- radio circle
- option label
- optional explanatory subtext

Behavior:

- only one option may be selected
- row click selects the radio option
- the parent card should summarize the active selection when collapsed if useful

### Button Row

Use for destructive or utility actions such as `Reset to defaults` or `Open settings file`.

Rules:

- buttons should not dominate the row visually
- prefer one compact right-aligned button
- destructive actions should be visually distinct, but still within the same surface system

## Internal Expanded Content

Inside an expanded card, use simplified rows instead of independent outer cards.

For a `Font` card, the internal order should be:

1. font family row
2. font style row if supported
3. font size row
4. preview panel

If style is not supported, omit it cleanly. Do not show placeholder rows.

### Internal Row Layout

Recommended measurements:

- left inset from card edge: `18 px` to `20 px`
- right inset from card edge: `18 px` to `20 px`
- label column minimum width: `160 px`
- control column width: same as global control width token
- divider between internal rows: `1 px`

Rules:

- internal rows do not need icons unless the setting benefits from one
- labels align consistently across all internal rows
- controls keep a common trailing edge

## Preview Panel

The reference screenshots show a centered text preview below font controls.

Use this as a dedicated preview surface inside the expanded font card.

Recommended measurements:

- top margin above preview: `16 px`
- minimum height: `84 px`
- corner radius: `8 px`
- horizontal padding: `20 px`
- vertical padding: `20 px`

Rules:

- preview content is centered horizontally
- preview background should be subtly different from the main card surface
- preview text should render with the currently selected font and size
- preview should update live as controls change

Suggested preview string:

`The sound of ocean waves calms my soul.`

## Spacing Rules

Use these as the default spacing contract.

- category heading to first card: `12 px`
- card to card within category: `8 px`
- category to category: `24 px`
- title to description inside text block: `2 px` to `4 px`
- header row to divider when expanded: `0 px`
- divider to first internal row: `0 px`
- last internal row to preview: `12 px` to `16 px`

The page should feel calm and ordered. Do not compress spacing to fit more controls above the fold.

## Typography

The screenshots suggest a restrained type hierarchy.

Recommended tokens:

- category heading: `20 px`, semibold
- card title: `15 px` to `16 px`, semibold
- card description: `12 px` to `13 px`, regular
- internal row label: `14 px` to `15 px`, regular or medium
- control text: `14 px` to `15 px`, regular
- state text (`On`, `Off`): `14 px` to `15 px`, medium

Rules:

- use one primary UI font for labels and controls
- keep descriptions on one line when possible
- truncate long trailing values rather than wrapping controls awkwardly
- use the selected editor font only inside the preview area, not for the surrounding chrome

## Color And Surface Tokens

These values are approximate dark-mode targets derived from the screenshots. They should be treated as starting points, not sacred values.

### Base Tokens

- page background: `#1F2229`
- category/card surface: `#2A2F39`
- raised/interactive control fill: `#3A3F47`
- preview surface: `#252A33`
- border color: `#3D434D`
- primary text: `#F2F4F7`
- secondary text: `#C4CAD3`
- tertiary/icon text: `#A2A9B3`
- accent blue: `#2AA8F2`
- accent blue hover: `#49B8F5`

### Usage Rules

- The page background should be only slightly darker than the cards.
- Borders should remain subtle and low-contrast.
- Use the accent blue only for active controls, focus, and selected toggles.
- Do not use the accent color for large fills unless the control is actively on.
- Description text should read clearly but remain visibly less prominent than titles.

## Icon Rules

Icons are part of the row identity and should stay understated.

Rules:

- use a consistent line-icon family
- icon size: `20 px` to `24 px`
- icon slot width: `28 px`
- icon color: tertiary text color by default
- do not use brightly colored icons for ordinary settings rows

## Control Specifications

### Toggle Switch

Recommended geometry:

- total width: `38 px` to `42 px`
- total height: `20 px` to `22 px`
- thumb diameter: `16 px` to `18 px`
- radius: pill

States:

- off: dark neutral track, light thumb
- on: blue track, light thumb
- hover: slightly brighter track
- focus: subtle outline or glow using accent color

Label behavior:

- show `On` or `Off`
- keep the label aligned with other trailing controls

### Dropdown / Combo Box

Recommended geometry:

- width: `160 px` to `190 px` for top-level rows
- height: `34 px` to `36 px`
- corner radius: `6 px` to `8 px`
- internal horizontal padding: `12 px`

States:

- default: neutral filled surface
- hover: slightly lighter surface
- active/focused: accent-colored caret or outline

Rules:

- selected text is left aligned
- caret is right aligned
- keep the control compact rather than wide and flat

### Radio Buttons

Recommended geometry:

- outer circle: `18 px`
- inner selected dot: `8 px`

Rules:

- align radios to the option label baseline
- make the whole row clickable
- preserve generous vertical spacing between radio options

### Buttons

Recommended geometry:

- height: `34 px` to `36 px`
- width: content-based, minimum `140 px`
- corner radius: `6 px` to `8 px`

Rules:

- default buttons use the neutral control fill
- only destructive buttons use a red-tinted state
- avoid oversized filled buttons inside settings cards

## Interaction Rules

### Hover

- top-level cards brighten slightly on hover
- controls may brighten independently
- hover should remain subtle; avoid strong glow effects

### Focus

- keyboard focus must be visible on controls
- use the accent color sparingly as a ring, stroke, or caret tint

### Expansion

- expansion should be immediate or near-immediate
- if animated, keep duration short: `120 ms` to `180 ms`
- do not animate large bouncy movements

### Live Apply

Settings changes should apply immediately unless the setting is explicitly staged.

Implications:

- dropdown changes update the app instantly
- toggles update instantly
- preview panels update instantly
- reset actions apply immediately after confirmation if confirmation is used

## Alignment Rules

These are critical. Most of the Notepad look comes from consistent alignment.

- all top-level card titles start on the same x position
- all top-level icons occupy the same slot width
- all trailing controls end on the same x position within a category
- internal expanded rows also share one trailing control edge
- divider lines extend across the content area, not the full page width

If alignment drifts, the page will stop feeling like the reference even if colors and spacing are correct.

## Content Writing Rules

- category titles should be short and plain
- row titles should be 1 to 3 words when possible
- descriptions should explain behavior, not implementation
- avoid technical storage details in the main visual rows unless the row is explicitly diagnostic

Examples:

- good: `Word wrap`
- good: `Open files`
- good: `Continue previous session`
- avoid: `Stored as TOML and loaded before session restore`

Implementation-specific or diagnostic detail can appear in a secondary section such as `Advanced`.

## Scratchpad Mapping

To make Scratchpad look closer to the reference, the current settings page should move from large always-expanded section cards to category headings plus individual cards.

Recommended top-level structure:

### Text Formatting

- `Font` as an expandable card
- `Word wrap` as a toggle card

Inside `Font`:

- `Family` dropdown row
- `Size` dropdown row
- preview panel

### Opening Scratchpad

- `Open files` as a dropdown card if this setting exists
- `When Scratchpad starts` as an expandable radio-group card if session startup behavior is exposed
- `Recent files` as a toggle card if this setting exists

### Diagnostics

- `File logging` as a toggle card

### Advanced

- `Settings file` as a static value card or button card
- `Reset to defaults` as a button card

This grouping is closer to the reference than placing multiple rows inside one large generic accordion section.

## Implementation Guidance For egui

### Recommended Component Split

If the page is refactored, use reusable view helpers for:

- `settings_category(...)`
- `settings_card(...)`
- `expandable_settings_card(...)`
- `settings_row_control_slot(...)`
- `settings_toggle(...)`
- `settings_combo(...)`
- `settings_preview_panel(...)`

### Layout Strategy

- Keep the current centered scroll-column approach.
- Replace broad section containers with per-setting cards.
- Use one shared card frame style and one shared inner row layout contract.
- Reserve `CollapsingHeader`-style behavior for individual cards, not whole categories.

### Current Code Implication

The current implementation in `src/app/ui/settings.rs` is structurally close enough to reuse, but these changes are needed to match this spec:

- introduce plain category headings outside card frames
- convert each top-level setting into its own card
- keep `Font` as the only expanded card in the first category
- move `Word wrap` out of the font card into its own top-level card
- simplify descriptive text so rows read like product settings, not developer notes
- add icon slots even when icon glyphs are placeholders initially
- make trailing controls share a stronger alignment contract

## Acceptance Criteria

The page satisfies this spec when all of the following are true:

- the page reads as a single centered scrolling column
- categories are labeled by plain headings outside the cards
- each top-level setting is represented by its own rounded card
- the `Font` card can expand inline without looking like a separate page
- trailing controls align consistently across peer rows
- toggle, dropdown, and radio behaviors share one visual language
- the overall page is immediately recognizable as visually similar to Win 11 Notepad settings in dark mode

## Non-Goals

- exact pixel parity with Microsoft's implementation
- exact typography parity with the system font stack
- exact icon parity with Microsoft's icon set
- reproducing proprietary assets

## Practical Tolerance

When implementing this spec, treat measurements as target ranges. A difference of `1 px` to `3 px` is acceptable if the final composition still preserves:

- calm spacing
- strong alignment
- low-contrast surfaces
- restrained typography
- compact right-aligned controls

If forced to choose between exact numeric fidelity and preserving the overall composition, preserve the composition.
