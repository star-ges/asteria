# Asteria: redesign and migration plan

*Omacraft → Asteria. Planning document, 19 November 2026. **Approved 19 November 2026** with the answers recorded in section H. Where an answer changed the original proposal, the affected section has been updated and says so.*

Sources inspected:

- **Omacraft**, cloned read-only to `/home/star/Projects/omacraft-source` (71 commits, `master` at `825f523`, tag `v0.1.0`). Its remote still points at `star-ges/omacraft`; nothing has been changed or pushed.
- **The `starshell-design` skill**: `~/.claude/skills/starshell-design/design-language.md` (read in full), plus `reference/hover-list-menu.png` and `reference/strong-lines-grid.png`.
- **StarShell itself**, to check how the principles were actually built: `~/Projects/StarShell/CLAUDE.md`, `docs/brief.md`, `shell/theme/Theme.qml`, `shell/components/*` (`HoverList`, `SettingGroup`, `SettingRow`, `SectionLabel`, `Tile`, `OutlinedButton`, `Caption`), `shell/bar/Wordmark.qml`, `shell/services/Appearance.qml`.

How design changes are labelled:

- **[S1] applies StarShell.** It follows from an existing principle or kit part.
- **[S2] extends StarShell.** A pattern the language doesn't have yet, built from its principles.
- **[S3] exception.** It would break a principle. Only proposed here, and needs your approval (section H).
- How deep the change goes: **cosmetic** (looks only), **structural** (layout or navigation), **behavioural** (what the app does).

---

## A. Current-state assessment

### A.1 Architecture

| Layer | What it is |
|---|---|
| Language / build | Rust 2024, one binary (`omacraft`). About 30k lines in `src/`, plus a vendored, patched `smithay-clipboard` for Wayland drag and drop. |
| UI | **egui / eframe 0.36** (wgpu, winit 0.30), immediate mode. Every widget is painted by hand through a small kit (`src/ui/widgets.rs`). |
| Async | tokio runtime. UI work runs as `Promise::spawn` jobs polled each frame. Long jobs carry a `Task`, which the status bar shows (`src/task.rs`). |
| Frontend ↔ backend | Direct function calls. The UI modules (`src/ui/*`) call the domain modules (`instance`, `launch`, `modrinth`, `backups`, `shares`, `bisect`, …) inside promises. There is no IPC or separate frontend. The **CLI** (`src/cli.rs`) uses the same domain modules. |
| Theme | `src/theme.rs` reads **Omarchy**'s `colors.toml` and `shell.toml`, plus Hyprland's border colour. Fonts come from fontconfig `monospace` with a bundled Nerd Font. Changes are picked up by polling. The fallback is Tokyo Night. |
| Keyboard | `src/keymap.rs` (every action, rebindable in `config.toml`) and `src/ui/nav.rs` (focus zones List/Main/Side, hints bar, `?` help). Mouse and keyboard share one code path (`|| key(Nav::X)`). |
| Persistence | `~/.local/share/omacraft/` (instances, store, backups, shared sets, accounts), `~/.config/omacraft/config.toml`, files written atomically. |
| Tests | Extensive. `AGENTS.md` ties dozens of behaviour rules (backups, shared folders, bisect, config edits, drops) to named regression tests. |

### A.2 Features (all kept; feature parity is a requirement)

- **Instances:** create, import (Prism/MultiMC/PolyMC folders or zips, `.mrpack` files or URLs, a Modrinth modpack browser), export (`.mrpack` or MultiMC zip), edit (name, group, share set, world set, memory, Java, JVM args, wrapper, window size), icons, notes, groups (ordered, coloured, collapsible, drag and drop), played time.
- **Launching:** vanilla, Fabric, Quilt, Forge, NeoForge. Mojang Java runtimes, QuickPlay (join a world or server directly), live game log, kill.
- **Accounts:** Microsoft (device code), offline, skins (upload, classic/slim, reset), capes, a 3D skin preview.
- **Content:** a Modrinth browser for mods, resource packs and shader packs with dependency resolution, a deduplicated content store with reflinks, mod updates and rollback, file drops.
- **Mod sets** and **find the breaking mod** (a bisect search with crash recovery).
- **Configs editor:** `options.txt`, mod TOML/JSON/JSON5 as fields or text, shader-pack options and profiles.
- **Worlds:** list, rename, duplicate, delete, open directly, backups and restores, play records, world sets. **Servers:** edit the list, join directly. **Screenshots:** per instance or across all of them.
- **Logs:** live and past logs, crash reports, a **crash analysis** panel that names suspect mods.
- **Shared folders:** share sets and world sets. **Reclaim disk space.**
- **CLI:** `list`, `launch`, `create`, `mod add`, `import`, `export`, `reclaim`, `theme`.

### A.3 UI structure today

```
┌ top bar ────────────────────────────────────────────────────────────────┐
│ ◧ Omacraft   1 instances  2 accounts  3 settings        ● running  👤 me │
├ sidebar (≤300px) ─┬ instance header: icon · name · meta · [play][edit][folder]
│ / filter      [+] │ version log mods resource packs shader packs configs notes
│ ▾ Group ──────  2 │ worlds servers screenshots log history   ← 11 tabs
│  ▣ Instance  1.21 │ ┌ tab body (often two columns) ───────────────────────┐
│  ▣ Instance  26.3 │ │                                                     │
├───────────────────┴─┴─────────────────────────────────────────────────────┤
│ [?] keys [j/k] instances [l] into tab …                      ▬▬▬ task …   │
└ bottom bar: key hints + current task progress ───────────────────────────┘
   + modal windows (2px accent border over a scrim) for 12 kinds of dialog
   + toasts, top right
```

### A.4 Strengths, kept on purpose

- **Real keyboard-first design** with one shared path for keys and mouse. This is exactly StarShell's "keyboard gets the same response" principle, and it's already built.
- **Square corners, no shadows, one filled button per screen, accent mostly for state.** The Omarchy idiom already shares a lot of DNA with StarShell. The redesign changes its character more than its discipline.
- **A centralised theme** (`Theme` tokens, `t.fg_a()`) and a **widget kit** that every screen goes through. That makes a token swap possible without touching 30k lines.
- **Old content stays up while refreshing** (worlds, mod sets, configs). This is "hold still", already done.
- **Careful, honest copy.** Explanations are written in plain sentences (bisect dialog, export options). It just needs the case convention changed.

### A.5 Weaknesses, judged against StarShell

1. **Type doesn't lead.** One monospace face at one size (≈13px), with headings only 1.6× that. Hierarchy comes from colour and boxes, not scale. Nothing is "big".
2. **Colour carries meaning StarShell gives to type.** Seven group hues as fills and borders, green "installed" and "running", yellow warnings, red danger, blue folder labels: about 75 direct uses of palette hues in `src/ui`. Selected rows get filled and bordered boxes.
3. **Boxes, not lines.** Rows are framed rectangles with washes (`row_frame`), inputs are dark wells, logs sit in a filled well, and dialogs float over a scrim with a 2px border.
4. **Navigation is wide and flat.** Eleven peer tabs in lowercase mono wrap onto a second line at the minimum width. "version", "notes", "servers" and "log history" carry the same weight as "mods".
5. **The instance list wastes the signature moment.** Choosing an instance is the app's main act of choosing, yet it's a dense 13px row list with tinted boxes.
6. **Transient things pop over the top.** Twelve modal types and top-right toasts, where StarShell lets transient content take over a space that already exists.
7. **Nothing responds with motion.** Hover is a faint wash, with no shift, reveal or stars.
8. **Omarchy coupling**, which has to go (inventory in F.3).

---

## B. The desktop design language

### B.1 Principles (from `design-language.md` §2, checked against StarShell's code)

| Principle | What it means for Asteria | Where StarShell does it |
|---|---|---|
| **Type carries the design** | Instance names, page titles and big figures (play time, disk size) in Melodrama Medium. Headings in Switzer ExtraBold 28/18, body in Switzer SemiBold 14, captions UPPERCASE 11px with 2px tracking. | `Theme.qml` `sizeDisplay 64`, `sizeHeading 28`, `sizeHeadingSmall 18`, `sizeBody 14`, `sizeCaption 11`, `captionTracking 2` |
| **Structure is drawn** | 1px hairlines shared between neighbours, a visible grid, radius 0 everywhere (images too), numbered sections `01 / MODS`. | `Rule.qml`, `SectionLabel.qml`, `SettingGroup.qml` (bracket hairline, a full-width rule means a new group) |
| **Calm by default, colour means state** | One surface and two text greys. The accent only for hover, focus, active and selected. Selection is a wash of `textStrong` at 6–16%, never an accent tint. Content (mod icons, screenshots, skins) is the only other colour. | `Theme.qml`: `error: accent`, `appWashOpacity 0.06`; no other hues outside the terminal and code palettes |
| **Restraint: one of everything** | Two typefaces, one icon set (Phosphor Regular, muted), one easing family. Icons support labels and never replace them, except play and stop. | `iconColor: textMuted`, `iconFamilyLight` |
| **The interface answers** | Hover and keyboard focus: title turns accent, shifts left 32px, an uppercase description slides in, ~5 stars fade in behind, a picture shows inline at the row's right end. 180ms OutCubic for colour, 280ms OutExpo for movement. | `HoverList.qml` and the hover tokens `hoverShift 32`, `descriptionDelay 60`, `imageryStagger 50`, `imageryDrift 18` |
| **Hold still** | Empty states are shown, not hidden. Transient content takes over an existing space. Lines align exactly. | Notepad slides over an existing side, the OSD replaces the media cell, the monitor confirmation is a strip within the page |
| **Fit the pattern to the job** | Choosing (instances, versions, modpacks, worlds, accounts) gets the hover list. Reading (logs, backups, stats, mod metadata) gets quiet tables with no hover. | The Load page's process table vs the launcher |
| **Honest interactions** | Only things that do something react. | |
| **Decide, don't add options** | No theme picker, no density setting. | |
| **Tune in one place** | Every value is a named token. Light and dark share token names and crossfade over 600ms. | `Theme.qml` header comment, `darkFadeDuration 600` |

### B.2 Deliberate conventions vs incidental details

- **Deliberate, carried into Asteria:** the palette values, the type pairing and scale, the hover response and its tokens, hairline structure and numbered sections, square everything, outlined buttons with a filled active state, settings groups opened by a big number and title, the bar's hairline cells with a 3px accent marker under the hovered or open cell (`barCellMarker`), dark mode following one switch for shell and apps.
- **Incidental, not carried:** Quickshell/QML mechanics, layer-shell panels, the specific panel sizes (`dropdownWidth 640`), the bar's 2px rule (that exists because the bar meets the wallpaper; Asteria's chrome meets only its own background, so 1px), StarShell's own star wordmark as such (Asteria needs its own mark, see C.1).
- **How StarShell talks to apps:** `Appearance.qml` writes GNOME's `color-scheme` (`prefer-dark` or `prefer-light`) to dconf, which the XDG desktop portal exposes to every app. That's the **standard freedesktop signal**, so Asteria follows it without depending on StarShell (see C.4).

---

## C. Proposed Asteria design direction

### C.1 Identity

**The idea: Asteria is a launcher that belongs to the same studio as StarShell.** Its identity comes from typography and one mark, not from a theme.

- **Wordmark:** "Asteria" in Melodrama Medium, sentence case, set large on the index page (C.3) and small (Switzer ExtraBold 18) in the bar. No logo lockup gimmicks. **[S1]**
- **Mark: a star in a block.** A four-pointed star (StarShell's star geometry, `starInnerRatio 0.22`) inscribed in a square hairline outline. The square is the one quiet Minecraft reference: a block, drawn as a line. It's static at rest; in the bar, hovering turns it accent and rotates the star 45° inside a fixed frame, echoing `Wordmark.qml` without copying it. **[S2]**
- **App icon** (`packaging/asteria.svg`), *as decided in H.4*: a square of the dark background (`#1c1a17`) with the four-pointed star in rust, shaded by a **vertical gradient that lightens towards the top** so it reads as shiny. Square corners. **The gradient is only on the desktop application icon.** Inside the app the mark stays flat (ink at rest, accent on hover). **[S3, approved]**
- **Astronomical motifs, deliberately limited to two:**
  1. **Stars appear only around what you're reaching for**, exactly as in StarShell (hover lists). That's the whole "space" vocabulary.
  2. **The name.** No starfield backgrounds, constellation lines, planet icons, "launch" rocket metaphors, or space-flavoured copy ("Lift-off!"). Copy stays plain.
- **No splash screen.** The app opens in well under a second, and a splash would be decoration with no job. The first frame is the index, with the instance list read synchronously as today. **[S1, decide]**

### C.2 Design system (tokens)

One `Tokens` struct replaces `Theme` (details in E.1). Values are StarShell's, so the family resemblance is exact:

| Group | Tokens |
|---|---|
| Colour | `background #fef0d5 / #1c1a17`, `accent #ad5034 / #f2683a`, `textStrong #202020 / #e2d6bf`, `textMuted #4c4c4c / #a99f8d`, `lineStrong`, `line` (light `textMuted`@45%, dark `#3a3630`), `wash` (`textStrong` @ 0.06 rest-selected, 0.12 pressed), `textOnFill = background` |
| Clarity hues *(approved exception, see C.6)* | `clay #a33b30 / #ee7266` (errors), `ochre #8a6516 / #aa843b` (warnings), `olive #566a2a / #7c9251` (running, installed, success), `sienna #94541f / #d68a4c`, `teal #2e6862 / #5a948d`, `slate #35596f / #698fa7`, `plum #7d4a63 / #b17a94`. Contrast on their own background is 4.7–6.6:1 (light) and 5.0–6.3:1 (dark). The olive, ochre, teal, slate and plum values are StarShell's own terminal colours |
| Type | `display` Melodrama Medium 64 (list titles), `displayLarge` ~120–168 (index wordmark, big figures, line height 0.8), `heading` Switzer ExtraBold 28, `headingSmall` 18, `body` Switzer SemiBold 14, `caption` 11 UPPERCASE +2 tracking |
| Space | unit 8, gutter 24, gap 12, line 1, radius 0 |
| Lists | `listRowHeight 104`, `listIndent 120`, compact `56 / indent 64`, `hoverShift 32 / 20`, description width 300/170, 3 lines |
| Motion | fast 180 OutCubic, medium 280 OutExpo, imagery 350 OutExpo, stagger 50, drift 18, start scale 0.92, panel rise 24, dark switch 600 |
| Controls | button height 36, padding 18, switch 36×20 inset 4, field height 44, slider 10 + ticks 5 × 10 |

### C.3 Navigation philosophy

**Two levels instead of a permanent master and detail: an Index you choose from, and Instance pages you work in.** Choosing is expressive, working is quiet.

```
┌ bar: ✦ ASTERIA │ 01 INSTANCES │ 02 ACCOUNTS │ 03 SETTINGS │ ······ │ ▪ RUNNING VANILLA+ │ STAR_GES ┐
│                                                                                                │
│  01 / SINGLEPLAYER ─────────────────────────────────────────────────────────────────────── 2  │
│ ──────────────────────────────────────────────────────────────────────────────────────────── │
│            Vanilla+                     FABRIC 0.19.3 · 1.21.11            ✦        [icon]    │  ← hovered: accent, −32px,
│ ──────────────────────────────────────  PLAYED 93H · LAST TODAY ─────────────────────────────  │    description, stars, icon
│            26.3                                                                                │
│ ──────────────────────────────────────────────────────────────────────────────────────────── │
│  02 / MODDED SERVERS ───────────────────────────────────────────────────────────────────── 2  │
│            Epic Modpack 1                                                                      │
├────────────────────────────────────────────────────────────────────────────────────────────── │
│ ENTER OPEN  P PLAY  N NEW  I IMPORT  / FILTER  ? KEYS                                          │
└────────────────────────────────────────────────────────────────────────────────────────────────┘
```

- **Index** (`01 INSTANCES`): the full-width hover list of instances, grouped under numbered section labels. `Enter`/click opens the instance page, `p` plays the highlighted one straight away, `/` filters (the query takes over the top of the list in large Switzer ExtraBold, like StarShell's launcher search, `sizeSearch 44`).
- **Instance page:** a **compact rail** (StarShell's compact hover-list variant, 56px rows) of the other instances on the left, so switching stays one key away. On the right, the instance header and its sections (D.3). `h`/`Esc` from the rail goes back to the Index.
- **Top-level destinations stay three** (Instances, Accounts, Settings), now drawn as bar cells. Their keys `1/2/3` are unchanged.
- **Transient things take over existing space** (C.5) rather than opening windows.

This is the main structural change. The conservative alternative and its trade-offs are in D.2 and H.1.

### C.4 Light, dark and the desktop

- **Follow the system's colour scheme** (the XDG portal `org.freedesktop.appearance color-scheme`, which StarShell drives via dconf). Crossfade every token over 600ms when it changes. **No in-app theme setting** ("decide, don't add options"); the system switch is the preference. **[S1]**
- **No Omarchy, Hyprland or StarShell files are read.** Asteria works the same on GNOME, KDE or anything with a portal; on a desktop without the setting it starts light (the original palette).
- **Window:** keep `decorated(true)` (Hyprland draws none; other desktops add their own) and set the Wayland app-id to `asteria` for window rules. The **opacity window rule** StarShell keeps for its themed apps can include Asteria with one line in the user's Hyprland config; Asteria doesn't write it (respect what isn't yours).
- **Fonts:** Switzer, Melodrama and Phosphor are found by family name through fontconfig. Fontshare's licence forbids redistribution, so they are **never vendored** in the public repo. Phosphor is MIT and **can** be bundled as the icon fallback, replacing the bundled Nerd Font. Fallback behaviour if Switzer or Melodrama are missing is decided in H.5.

### C.5 Overall experience: the four kinds of surface

| Surface | Pattern | Used for |
|---|---|---|
| **Choose** | Hover list (full or compact): large type, accent, shift, description, stars, inline picture | Instances, accounts, versions, modpacks, worlds, servers, capes, log files, bisect answers |
| **Read** | Quiet table: hairline rows, uppercase tracked headers, tabular figures, no hover | Live log, backups, mod metadata, crash evidence, components, process/task details |
| **Set** | Settings groups: big number and title, bracket hairline, label left, control right | Settings, configs editor fields, edit instance, export options |
| **Take over** | A **sheet** that takes over the content area beneath the bar (content rises 24px, 280ms OutExpo; leaving fades back), or a **strip** inside a pane for confirmations | New instance, import, export, edit, version change, rollback, mod sets, bisect, confirm, sign-in code |

### C.6 Clarity hues (approved exception)

*Decided in H.2.* With no hues at all, a launcher full of logs, mod lists and statuses becomes a wall of text. So Asteria gets a small **terracotta palette for clarity and readability only** (tokens in C.2), on the same footing as StarShell's code colours: they're there because reading needs them, not as a design element.

- **Allowed:** text colour and small marks. Examples: a group's label and its square marker; the level caption and text of error and warning log lines; status captions (`RUNNING` olive, `INSTALLED` olive, `ERROR` clay); crash confidence (`CERTAIN` clay, `LIKELY` ochre); a 3px tick at the left edge of a row whose group has a colour.
- **Never:** fills, washes, tinted rows, borders around boxes, gradients, or anything that marks hover, focus or selection. Those stay the accent and the `textStrong` wash, so "accent means this one" still holds.
- **Errors are clay, not the accent.** Clay is redder and deeper than rust, so an error never looks like a hovered item.
- The seven group colour names stored in `groups.toml` map onto the hues: red → clay, orange → sienna, yellow → ochre, green → olive, cyan → teal, blue → slate, magenta → plum. Existing data keeps working and round-trips with Omacraft.
- Recorded in Asteria's `AGENTS.md` under "Approved exceptions", so nobody later "fixes" it.

Copy conventions **[S1]**: captions, section labels, tabs and button labels are written in sentence case in the source and **rendered uppercase with tracking** (one function, so translation or tone changes stay in one place). Headings use sentence case ("Find the breaking mod"). Body text stays as it is written today (sentence case, plain). "Instance", "world set" and "share set" keep their names.

---

## D. Screen-by-screen redesign

Each screen lists its **adaptation** (the existing layout in StarShell's language) and its **reimagination** (designed from scratch with StarShell), then a recommendation.

### D.1 App shell: top bar, bottom bar, toasts

**Today:** a mono top bar (accent "Omacraft", `1 instances`… tabs with an accent underline, a green "● running" list, the account). A bottom bar of boxed key hints plus the current task. Toasts top right with 2px borders, red for errors.

**Works:** one-glance state (account, running games, task), keys always visible.

**Doesn't:** colour as status (green running, yellow "no account", red error border), boxed key caps, toasts float over content.

**Adaptation:** restyle in place: Switzer, captions, no hues.

**Reimagination (recommended):** StarShell's **bar of hairline cells**, top and bottom.

- **Top bar** (48px): `[✦ Asteria] │ 01 INSTANCES │ 02 ACCOUNTS │ 03 SETTINGS │ ··· │ RUNNING VANILLA+ ▪ │ STAR_GES MICROSOFT`. The open cell gets the 3px accent **marker** at its foot; hovering any cell shows the marker and turns its label accent. The running cell shows a small olive square and an olive `RUNNING` caption (C.6), and opens that instance's log. "No account" is an ochre caption whose cell, like any cell, turns accent on hover and leads to Accounts. **[S1]** structural (cosmetic plus the cells now being targets).
- **Bottom bar:** key hints as `ENTER OPEN  P PLAY` captions: the key in `textStrong`, the label muted, no boxes. **[S1]** The task cell on the right: stage text plus a **thin meter with a ruler of ticks** (kit slider/meter). Indeterminate progress keeps a moving block (it's real activity, so motion is honest).
- **Notices replace toasts:** a notice **takes over the bottom bar's hint cell** for 5s (info) and fades back. Errors stay until dismissed (`Esc` or a click) and are marked by a clay square plus a clay `ERROR` caption (C.6). A long error shows its first line with `MORE`, which opens it in a sheet. **[S2]** behavioural (where messages appear and how long errors last).

### D.2 Instance list → Index and rail

**Today:** a 200–300px sidebar: filter plus `+`, group headers (`▾ name ─── n`, tinted by group colour), rows boxed and tinted by group colour, focus marked with a 3px accent bar, drag and drop between groups, context menu.

**Works:** fast keyboard selection, grouping, drag and drop, filter.

**Doesn't:** the main choosing act is drawn as the densest, most boxed part of the UI; group colours add up to seven hues.

| | Adaptation (conservative) | Reimagination (recommended) |
|---|---|---|
| Layout | Keep the sidebar. Rows become compact hover-list rows (56px, Switzer 20, shift 20, small stars). Group headers become `01 / SINGLEPLAYER ───── 2` captions. | **Index page:** full-width hover list (104px rows, Melodrama 64). Description: `FABRIC 0.19.3 · 1.21.11 / PLAYED 93H · LAST TODAY / 46.6 GB`. Inline picture: the instance icon. **Instance page:** the compact rail from the adaptation. |
| Cost | Low | Medium: a new "Index" view state. List, group and drag code is reused by both. |
| Feel | StarShell-styled launcher | Feels like it started from StarShell: opening Asteria is a portfolio index of your worlds |

- **Groups keep their colour as a clarity hue** *(changed by H.2)*: the numbered section label's text and a small square marker take the group's hue (C.6), and each row in the group gets a 3px tick at its left edge. There are no tinted fills or borders. The group manager's colour picker stays, offering the seven terracotta hues. **[S3, approved]**
- **Drag and drop between groups** stays. While dragging, the target section's rule turns accent: an honest drop marker. **[S2]**
- **Filter:** typing `/` turns the list head into a large query line (Switzer ExtraBold 44) with a 2px caret, like StarShell's launcher. Rows that don't match are hidden, and the group labels of empty groups stay with a count of 0 ("hold still"). **[S1]**
- **Empty state (no instances):** the wordmark "Asteria" set very large in Melodrama, a one-line caption ("AN INSTANCE IS A SELF-CONTAINED MINECRAFT INSTALL"), and two outlined buttons `NEW INSTANCE` / `IMPORT`, with the first filled. **[S1]**

### D.3 Instance page: header and sections

**Today:** a 62px rounded icon, the name at 21px, a meta line (minecraft · loader · blue folder group · share set · world set), a stats line, and play/edit/folder buttons. Below that, 11 tabs.

**Reimagination (recommended):**

```
│ 01 / INSTANCE                                              FABRIC · 1.21.11 │
│ Vanilla+                                         [ PLAY ] [ EDIT ] [ FOLDER ]│   ← Melodrama 64, PLAY filled
├──────────┬──────────┬──────────────┬──────────────┬───────────┬─────────────┤
│ MINECRAFT│ LOADER   │ GROUP        │ SHARE SET    │ WORLD SET │ PLAYED      │   ← spec strip: caption over
│ 1.21.11  │ 0.19.3   │ Singleplayer │ Singleplayer │ —         │ 93h 9m      │     value, shared hairlines
├──────────┴──────────┴──────────────┴──────────────┴───────────┴─────────────┤
│ OVERVIEW   CONTENT   CONFIGS   WORLDS   SCREENSHOTS   LOGS                  │   ← section tabs, accent marker
├─────────────────────────────────────────────────────────────────────────────┤
│ MODS · RESOURCE PACKS · SHADER PACKS                                        │   ← sub-tabs where a section has them
```

- **Name** in Melodrama 64 (elided on one line). The icon leaves the header: it's content imagery and shows inline in the rail and Index on hover. Clicking the name's caption `ICON` (or `I`) opens the icon sheet. **[S1]** cosmetic/structural.
- **Spec strip:** a row of hairline cells (the strong-lines grid), each a caption over a value. It's read-only. Group, share set and world set cells are honest links to Edit (hover: accent and marker). Disk size is in a cell and says `MEASURING` until it's known. **[S1/S2]**
- **PLAY:** the filled button (`textStrong` at rest, accent on hover), `STOP` in the same slot while running. Play keeps a Phosphor Bold play glyph because there the icon is the label (kit rule). **[S1]**
- **Sections: 11 tabs → 6**, keeping every view: **[S2] structural**

| Section | Sub-tabs | Was |
|---|---|---|
| 01 Overview | | `version` (components and loader) + `notes` |
| 02 Content | mods · resource packs · shader packs | three tabs |
| 03 Configs | minecraft · mods · shaders | `configs` (already had these) |
| 04 Worlds | worlds · servers | two tabs (both are "where you play": both QuickPlay) |
| 05 Screenshots | | `screenshots` |
| 06 Logs | live · history | `log` + `log history` |

  `[`/`]` still step through every leaf view in order, so muscle memory carries over. `c` (go to a mod's configs) and other jumps target leaves as now. The `InstTab` enum stays as the leaf identity; sections are a grouping over it. **Conservative alternative:** keep 11 flat tabs as uppercase captions with the accent marker. It's cheaper, but wraps to two lines at 760px, which breaks the grid. Decision in H.1.
- **The mod-sets / bisect banner** (shown while a search runs) becomes a **strip under the spec strip**: caption `SEARCH IN PROGRESS · STEP 3 OF ~7`, with buttons on the right. It's an existing slot, so no layout jump appears when it's absent; it's simply not drawn. **[S1]**

### D.4 Overview (version and components, notes)

**Today:** a "components" list (minecraft, loader with logo) plus a `change version…` button; notes as a separate tab with a text area.

**Proposal:** two columns sharing a vertical hairline. Left: **components as a quiet table** (`COMPONENT / VERSION`, tabular figures, loader logos small and muted beside names), `CHANGE VERSION` outlined. Right: **notes** as a plain text area with no well: a 1px frame, Switzer 16, line height 150% like StarShell's notepad, saved as you type (behaviour unchanged). **[S1]** structural (two tabs merged into one view).

The loader logos (Fabric, Quilt, NeoForged) are licensed "only to label the loader, unmodified", so they stay as they are, small, with no tinting. That's content, allowed.

### D.5 Content: mods, resource packs, shader packs, Modrinth

**Today (see `docs/screenshots/Title Screenshot.png`):** left, installed items as two-line rows (icon, name, version · size · file) with a cog to jump to configs; top buttons `folder`, `check updates`, `roll back…`, `72/72 enabled`. Right, a persistent Modrinth search with result cards and a green "installed".

**Works:** installed and available side by side, which suits adding mods while seeing what's there. Keep the two-pane composition.

**Doesn't:** two heavy lists compete for attention, the green label, and the boxed rows.

**Proposal (adaptation, it's the better result here):**
- **Left, installed:** a dense list (it's both choosing and managing). Use **compact hover rows** (56px): name in Switzer ExtraBold 18, a muted caption line `0.6.1 · 71 KB`, the mod icon 32px square at the left (content; square-cropped, radius 0), a **square switch** at the right for enabled/disabled. Hover shifts 20px and reveals the file name as the description. The cog becomes a `CONFIGS` caption link that appears on hover (honest, since it leads to the file). Disabled mods are muted, with their switch off. **[S1/S2]**
- Toolbar → a caption row: `01 / MODS · 72 OF 72 ON` left, outlined joined buttons `FOLDER │ CHECK UPDATES │ ROLL BACK` right. **[S1]**
- **Right, Modrinth:** the query in a large field (Switzer ExtraBold 28) with results as **hover rows**: title, `BY JELLYSQUID3 · 248M DOWNLOADS` caption; the description reveals on hover with the project icon inline at the right end. "Installed" becomes a muted `INSTALLED` caption plus a check glyph; `Enter` installs as today. **[S1]**
- **Mod updates:** found updates list as a quiet table (`MOD / FROM / TO`) inside a strip that takes over the top of the left pane. **[S1]**
- **Drop hint** (dragging files over the window): the target pane's frame turns accent with the caption `DROP TO ADD TO MODS`, instead of an overlay. **[S1]**

### D.6 Mod sets and Find the breaking mod

**Today:** a sets dialog (checklist), a bisect start dialog (see `docs/screenshots/find the breaking mod.png`: a blue border, a yellow warning paragraph, the list of mods with `test` labels), step question dialogs, a banner.

**Proposal:** both become **sheets**.
- **Sets:** left, the sets as a compact hover list (choose); right, the selected set's mods as a checklist of quiet rows with square checkboxes. "Required by…" flags become a muted caption with an accent square on the row that needs attention. **[S1]**
- **Bisect start:** the heading "Find the breaking mod" (Switzer ExtraBold 28); the explanation as body; the world-safety warning as a **bracketed paragraph** (a 1px vertical hairline to its left, `textStrong`, caption `BEFORE YOU START`), not yellow. The mod list's per-row state (`TEST`/`KNOWN FINE`/`KEEP ON`) is a joined three-way segmented control per row. **[S1/S2]**
- **Each step's question** is the expressive moment: `STEP 3 OF ~7` caption, then two hover rows in Melodrama: **"It happened"** / **"It didn't"**, with descriptions ("the suspects are among the 9 that were on"). A click selects; `ANSWER` sends (that guard against slips exists today and stays). **[S2]** cosmetic/structural, behaviour unchanged.

### D.7 Configs editor

**Today (see `docs/screenshots/config editor.png`):** minecraft/mods/shaders segmented; a file tree with mod icons; a field editor with name, description and control rows; buttons folder · restore defaults · fields/text · revert · save.

**Works:** the three-pane structure, field rows with descriptions, and save/revert discipline. **Adaptation is the right call: keep the layout.**

**Proposal:**
- File tree: compact rows (no hover shift: it's a navigator with up to hundreds of entries, where a 20px shift on every move is noise; the accent title and focus marker are enough). Mod groups as `MOD NAME   3` with the count. **[S2]** (a deliberate "quiet" choosing list for very long trees).
- Field rows → StarShell **SettingRow**: the key in Switzer SemiBold 14 `textStrong`, the description muted below it (max width 560), the control right-aligned on a shared column line so every switch and field aligns exactly. Rows are split by inset hairlines; a section header inside a file becomes a SettingGroup header (`02 / HUD`). **[S1]**
- Controls: square switch, outlined field (44px), slider with ruler ticks (range from comment hints, marks for the default), joined segmented for enums. **[S1]**
- Edited-but-unsaved rows: a short accent tick at the row's left edge (the marker idiom), and the header caption reads `2 UNSAVED CHANGES`. `SAVE` is filled only when there's something to save. **[S2]**
- Text mode: Switzer, not monospace (see H.6). Syntax colours, if wanted, reuse StarShell's code palette (berry, green, violet, blue, teal, comment), which is the language's own sanctioned exception. **[S1]**
- Key-binding capture (`config::capture`): the row's control turns into `PRESS A KEY · ESC CANCELS` in accent while waiting. **[S1]**

### D.8 Worlds and servers

**Today:** two columns: worlds (icon, name, last played, mode) | backups pane; servers as a list with ↑/↓, edit, join.

**Proposal:**
- **Worlds:** a compact hover list (choosing): world name Switzer ExtraBold 20; description `SURVIVAL · LAST PLAYED 2 DAYS AGO · 1.21.11 · FABRIC`; the world's own `icon.png` inline at the right on hover (content). `Enter` = play into the world (QuickPlay on 1.20+), as now. Worlds that only have backups left are listed below a `GONE · BACKUPS ONLY` section label. **[S1]**
- **Backups:** a **quiet table** (`TAKEN / KIND / SIZE`, tabular figures, `AUTO` or `BY HAND` captions). Selection is a `textStrong` wash. Restore and delete are outlined buttons under the table, and confirmation is a strip in the same pane ("Restore *World* to 18 Nov 21:04? What's there now is backed up first. [RESTORE] [CANCEL]"). **[S1/S2]**
- **Servers:** the same compact hover list (name; `PLAY.EXAMPLE.COM` description). Editing happens **inline**, the row expanding into two fields rather than a dialog. Reordering stays on keys (`MoveUp`/`MoveDown`) with ↑/↓ buttons as Phosphor arrows. **[S1]** structural (inline edit).

### D.9 Screenshots

**Today:** a thumbnail grid, an "all instances" toggle, open / copy / folder / delete.

**Proposal:** a **tile grid with shared hairlines** (like StarShell's wallpaper grid, `settingThumbColumns`): image cells edge to edge, a caption line under each (`2026-11-18 21:04 · VANILLA+` in merged mode). Hover: the caption turns accent and the 3px marker appears at the cell's foot. Selecting opens the picture **inside the section** (it takes over the grid area, with a 700ms crossfade between pictures and arrow keys stepping), not in a window. The "all instances" toggle becomes a square switch in the section caption row: `ALL INSTANCES [■]`. **[S1/S2]**

### D.10 Logs and crash analysis

**Today:** the live log in a dark well, level-coloured (accent, fg, yellow, red, muted); history as a file list plus viewer; a crash analysis panel with suspects and confidence.

**Proposal: logs are for reading, so quiet.**
- **Log view** *(changed by H.2 and H.6)*: no well; a 1px frame. Lines are set in **JetBrains Mono** (approved exception: raw text is easier to scan in monospace). A narrow left column carries the **level as a caption** (`WARN`, `ERROR`). Error lines are **clay**, warnings **ochre**, info `textStrong`, debug `textMuted`, and Asteria's own lines `textStrong` with an `ASTERIA` caption. **[S3, approved]**
  *Note on focus:* the accent for a focused pane frame is kept for actual keyboard focus only; a pane having focus is a real state, so the accent stays legitimate there.
- **History:** log files as a compact list (choose) on the left; the viewer on the right (read).
- **Crash analysis:** a **headline in large type** ("Sodium is the likely cause", Switzer ExtraBold 28, or Melodrama when it names one mod), a caption `CERTAIN / LIKELY / POSSIBLE` (clay / ochre / muted), then the evidence as quoted lines in a bracketed block, and actions `DISABLE SODIUM │ FIND THE BREAKING MOD │ COPY SUMMARY`. **[S1]**

### D.11 Accounts and skins

**Today (see `docs/screenshots/accounts and skins.png`):** signed-in accounts as rows, a filled blue "sign in with microsoft", offline username plus add, and a skin pane (the 3D viewer in a filled well, model classic/slim, upload, reset in red, a cape list).

**Proposal:**
- **Accounts:** a full hover list (Melodrama names). Description `MICROSOFT · ACTIVE`; the active one is marked with the accent square; `Enter` makes it active. Under the list, outlined `SIGN IN WITH MICROSOFT` and an offline name field with `ADD OFFLINE`. **[S1]**
- **Microsoft sign-in:** the device code shown **huge in Melodrama** in a strip that takes over the add area ("GO TO MICROSOFT.COM/LINK AND ENTER  **AB12-CD34**", with `COPY` and `OPEN`). This is the one place a big figure is genuinely useful. **[S2]**
- **Skin:** the 3D viewer framed by hairlines on the background (no well), a caption `DRAG OR , . TO TURN`; model as joined `CLASSIC │ SLIM`; `UPLOAD SKIN`, `RESET` outlined (no red: reset is confirmed in a strip). **Capes:** a compact hover list with the cape texture inline on hover. **[S1]**

### D.12 Settings

**Today:** one scrolling page with sections: appearance (Omarchy theme and font), java, versions, microsoft sign-in, storage, backups, shared folders, keys.

**Proposal: StarShell's settings page, nearly verbatim.** Numbered **SettingGroups** with the bracket hairline: `01 Java` (memory slider with ruler, Java path, JVM args, wrapper), `02 Versions` (show snapshots switch), `03 Backups` (keep, a stepper), `04 Shared folders` (share sets and world sets as quiet tables with member counts, delete when unused), `05 Storage` (paths as captions with `OPEN`, `RECLAIM DISK SPACE`, the result as a big figure "2.3 GB reclaimed"), `06 Sign-in` (client id: "use default"), `07 Keys` (action / chords table, capture in place).
- **Appearance section removed** (no theme or font to show: the system decides). **[S1]** behavioural (Omarchy information removed).

### D.13 New instance, import, version picker, icon picker, groups, export, edit

**Today:** modal windows with segmented mode switches, two columns (form | picker).

**Proposal:** all become **sheets** over the content area, keeping their two-column logic:
- **New instance:** left, a SettingGroup form (`NAME`, `GROUP`, `COPY SETTINGS FROM`); right, the **version picker as a hover list** (compact; `1.21.11` titles, `RELEASE · 2025-12-09` descriptions, the `SNAPSHOTS` switch in the caption row); the loader as joined `VANILLA │ FABRIC │ QUILT │ FORGE │ NEOFORGE` then a loader-version compact list. **[S1]**
- **Import → Modrinth:** the prime hover-list case (see `docs/screenshots/import.png`): pack titles in Switzer ExtraBold 28 (Melodrama 64 is too big for long names like "COBBLEVERSE - Pokemon Adventure [Cobblemon]"), the description `BY LUMYVERSE · 7.2M DOWNLOADS` plus summary, and the **pack icon inline at the row's right end** on hover. **[S1]**
- **Import → files and launchers:** detected launchers as a compact list grouped by `PRISM` / `MULTIMC` labels; "from a file, folder or link" as a field plus `BROWSE`. **[S1]**
- **Icon picker:** a tile grid of icons (shared hairlines; Prism's pixel art is content), `ADD IMAGE`, `REMOVE`. **[S1]**
- **Groups manager:** a compact list with rename, reorder and delete. The colour column offers the seven clarity hues as small squares (H.2). **[S1/S3 approved]**
- **Export:** format as joined `MODRINTH PACK │ INSTANCE ZIP`, each with its explanation as body text; included folders as a checklist with sizes in a quiet column. **[S1]**
- **Edit instance:** SettingGroups `01 Instance` (name, group), `02 Sharing` (share set, world set with explanations), `03 Game` (memory, Java, JVM args, wrapper, window). The warning when changing a world set becomes a bracketed paragraph. **[S1]**

### D.14 Confirmations, empty and loading states

- **Destructive confirms** (`Modal::Confirm`): a **strip** that takes over the bottom of the current pane: a sentence in `textStrong`, then `[DELETE]` (filled `textStrong`, accent on hover) and `[CANCEL]`. `Enter`/`Esc` as now. The enum and `ConfirmAction` are unchanged. **[S2]** structural.
- **Empty states:** the title large (Switzer ExtraBold 28), the hint as body, the action as an outlined button, all left-aligned to the grid (not centred with a giant glyph). "Hold still": the section header and tabs stay. **[S1]**
- **Loading:** keep the old content while refreshing (existing rule); when there's nothing yet, a muted caption `READING WORLDS` in the place the list will occupy. No spinners. **[S1]**

---

## E. Component and architecture changes

### E.1 Tokens and theme (`src/theme.rs` → `src/design/tokens.rs`)

- `Palette { background, accent, text_strong, text_muted, line_strong, line, wash(a) }`, with `LIGHT` and `DARK` constants (StarShell's values, see C.2) and `Tokens::palette()` returning the **crossfaded** palette (`darkness` 0…1 animated over 600ms, `lerp` per colour, the same as `Theme.shade()`).
- Type, space, list and motion tokens as `const`s in one file, one comment per value, as `Theme.qml` does.
- `apply(ctx)` keeps setting egui `Visuals`/`Style` (radius 0, no shadows, selection = `wash`, hyperlink = `text_strong` with an accent hover), so stock egui widgets that remain (TextEdit, ScrollArea, DragValue) inherit the look.
- **Removed:** `Theme::load_from` (Omarchy), `shell.toml` tokens, `border_active`, the Tokyo Night fallback and its test.
- **Clarity hues** (C.6) replace the 7 Omarchy palette hues under semantic names (`error`, `warning`, `ok`) plus the group hues. `Tokens::named("red")` keeps working for stored group colours. The ~75 callers of `t.red/yellow/green/…` move to the semantic names, and any that tinted a fill or border lose it.
- **Colour-scheme watcher:** read the portal setting at start and watch it. Prefer egui/winit's own `system_theme` if it reports Wayland changes in eframe 0.36 (to verify at the start of Phase 2). Otherwise a small D-Bus read of `org.freedesktop.portal.Settings` (`zbus` 5.19 is already in `Cargo.lock`, pulled in by `rfd`'s xdg-portal feature, so it adds no new heavy dependency). Polling the way the old watcher did is the last resort.

### E.2 Fonts (`src/design/fonts.rs`)

- Resolve **Switzer SemiBold**, **Switzer ExtraBold**, **Melodrama Medium** and **Phosphor** (Regular and Bold) via `fc-match` with family and weight, OTF/TTF only (egui's existing constraint). Register as egui families `text`, `heading`, `display`, `icons`, `icons-bold`.
- **Phosphor Regular and Bold bundled** (MIT) as the fallback for `icons`, replacing `SymbolsNerdFontMono`.
- **JetBrains Mono** for raw text (log lines, crash-report quotes, the config editor's text mode) *(H.6)*. Found via fontconfig, with JetBrains Mono Regular (OFL 1.1) **bundled** as the fallback, so logs look right on any system. The OFL allows bundling as long as its licence file ships with it.
- **Missing Switzer or Melodrama** *(H.5)*: fall back to fontconfig `sans-serif`, and Settings shows a one-line notice linking to Fontshare.
- **egui has no OpenType features**, so there are no real tabular figures. Where numbers change in place (progress, counters, the meter), lay digits out in fixed-width cells (the width of `0`). A small `tabular()` text helper does this. It's a known constraint, not a design change.
- Letter-spacing for captions via `TextFormat::extra_letter_spacing` (supported in egui 0.36).

### E.3 Widget kit (`src/ui/widgets.rs` → `src/design/kit/*`)

| New primitive | Replaces | Notes |
|---|---|---|
| `Rule`, `VRule` | ad-hoc `hline`/`vline` with `fg_a(0.12)` | `line` / `line_strong`, pixel-snapped so lines meet exactly |
| `Caption`, `SectionLabel(n, label)` | `section()`, muted labels | uppercase, tracked; `01 / LABEL ───── aside` |
| `Heading`, `Display` | `heading()` | Switzer ExtraBold 28/18; Melodrama sizes |
| `HoverList` + `HoverRow` (full, compact, quiet) | `menu_row`, `lead_row`, `drag_row`, `rich_row*`, `group_row`, `focus_mark` | one state per row: `rest/hot/selected`, animated with `ctx.animate_value_with_time` (shift, colour, description alpha and offset, star alpha/scale with stagger, inline picture fade). **Hot = hovered or keyboard-current**, from the same `Nav` path |
| `StarField` | none | 4/5-point star polygons (`starInnerRatio 0.22`), ~5 per row (3 compact), positions seeded from the row id so they don't jump, painted **behind** text and lines |
| `Button`, `ButtonRow` (joined) | `button`, `primary_button`, `danger_button`, `segmented` | outlined, uppercase; `filled` variant; no danger variant |
| `Switch`, `Checkbox` | egui checkbox / custom toggles | square track and knob, 280ms slide |
| `Field` | `text_input` | 1px frame, 44px, caret 2px accent; no well fill |
| `Slider`/`Meter` with ruler | egui Slider, `progress()` | 10 intervals of ticks; second-value marks (defaults) |
| `SettingGroup`, `SettingRow` | `field_label`, `kv` | bracket hairline; label left, control on a shared column |
| `Table` (quiet) | ad-hoc rows in logs and backups | uppercase headers, tabular numbers, wash selection, no hover |
| `Tiles` grid | screenshots and icon grid code | shared lines, caption under, marker on hover |
| `Bar`, `BarCell` | `top_bar`, `status_bar` | 3px accent marker for open or hovered |
| `Sheet`, `Strip` | `modal_frame`, `scrim`, egui `Window` modals, toasts | sheet = content area takeover with a 24px rise; strip = an in-pane confirm or notice |
| `Icon(name)` | ~57 Nerd Font `\u{f…}` escapes | name → Phosphor codepoint table (`glyphs.rs`), like StarShell's `Glyphs.js` |
| `motion` helpers | none | `ease_out_cubic`, `ease_out_expo`, `animate(ctx, id, target, Motion::Medium)`; repaint requested only while a value is moving ("nothing moves when you aren't doing anything") |

All new primitives register **AccessKit** info (role and label), so custom-painted rows stay visible to screen readers. eframe already builds with `accesskit_winit`.

### E.4 Application structure (`src/ui/*`)

- **`View` state** gains `Index` vs `Instance(id)` (C.3). `Tab` (top-level) stays. `InstTab` stays as the leaf enum; a new `Section` groups leaves (D.3). `[`/`]` keep stepping leaves.
- **Modals → sheets:** the `Modal` enum and each dialog's state and logic are **unchanged**; only `show_modal` changes from an egui `Window` over a scrim to drawing inside the central area. `modal_nav()` key routing stays as it is, so panes under a sheet still can't take its keys.
- **Toasts → notices** in the bottom bar (D.1): `App::toast()` keeps its signature; only rendering and lifetime rules change.
- **Groups colour** ignored in UI; `groups.rs` keeps the field for round-trips.
- **Responsiveness:** the minimum window stays 760×480. Below ~960px wide the rail collapses to the Index (instance pages full width, `h`/`Esc` back). Below ~600px tall the full hover list switches to its compact variant. Display sizes scale down one step (64 → 44) when the name column is under 480px.
- **Reduced motion:** honour GNOME's `enable-animations` through the same portal read (StarShell's dconf world). When off, durations collapse to 0 and stars don't appear. That's an accessibility signal, not a new in-app option.

### E.5 What doesn't change

Every domain module: `launch`, `mojang`, `loaders`, `java`, `auth`, `skins`, `modrinth`, `content_meta`, `store`, `import`, `export`, `saves`, `backups`, `shares`, `world_sets`, `mod_sets`, `bisect`, `crash`, `mod_config`, `vanilla_options`, `shader_config`, `fsutil`, `net`, `task`. They change only in name strings and the compatibility shims listed in F. **All behaviour rules in `AGENTS.md` and their tests stay in force.** Keymap actions stay; new defaults are added only for new navigation (Index ⇄ instance), and conflict detection (`keymap.rs`) covers them.

### E.6 Optional architectural improvement (separate, not required)

`src/ui/mod.rs` (1394 lines) and `src/ui/config.rs` (2327) mix layout with state. Splitting rendering from state per screen (`state.rs` / `view.rs`) would make the redesign phases easier to review, but it's a refactor with regression risk across the GUI-level tests (`ui::*::tests`). **Recommendation: don't do it up front.** Split a file only when its screen is being redone anyway, keeping the tests green per phase.

---

## F. Rebranding and migration

### F.1 Repository separation (procedure, needs approval to run)

1. **Source:** `/home/star/Projects/omacraft-source` (fresh clone, `origin` = `star-ges/omacraft`). Nothing is pushed from it, ever.
2. **Create Asteria's working copy with full history**, without touching the planning folder's files:
   ```sh
   git clone --no-local /home/star/Projects/omacraft-source /home/star/Projects/asteria.new
   cd /home/star/Projects/asteria.new
   git remote remove origin                    # points at the local clone; drop it
   git remote -v                               # must print nothing
   git tag omacraft-v0.1.0-fork 825f523        # mark the fork point (optional)
   mkdir -p docs/asteria
   cp ../asteria/REDESIGN_BRIEF.md ../asteria/ASTERIA_REDESIGN_PLAN.md docs/asteria/
   ```
   Then swap folders: `mv ../asteria ../asteria.planning && mv ../asteria.new ../asteria`. The planning folder is kept until you've checked the result.
3. **Optional read-only reference to Omacraft:** `git remote add omacraft https://github.com/star-ges/omacraft.git && git remote set-url --push omacraft DISABLED`. It can fetch upstream fixes and **can't push**.
4. **Branch name:** keep `master` or rename to `main` (H.8).
5. **New GitHub repo** (only with your explicit go-ahead): `gh repo create star-ges/asteria --private` (or `--public`), `git remote add origin https://github.com/star-ges/asteria.git`, then **verify** `git remote get-url --push origin` ends in `/asteria.git` before `git push -u origin <branch> --tags`.
6. **Before any public push:** confirm no Fontshare fonts are tracked (`git ls-files | grep -iE 'switzer|melodrama'` must be empty, and the history must have none either). Omacraft's history has none today.
7. **Afterwards:** delete `/home/star/Projects/omacraft-source` and `asteria.planning` once you've confirmed.
8. **Omacraft stays as it is.** Archiving it or adding a "continued as Asteria" note would need a push to it, so that's out of scope unless you ask for it separately.

### F.2 Naming inventory

**Cosmetic (safe to replace):**

| Where | Today | Asteria |
|---|---|---|
| Window title, `run_native` name | `"Omacraft"` (`src/main.rs`) | `"Asteria"` |
| Top-bar wordmark | `"󰍳 Omacraft"` (`src/ui/mod.rs:685`) | mark plus "Asteria" |
| UI copy | `ui/export.rs:124,132`, `ui/mod_sets.rs:943,964`, `ui/settings.rs:78`, `ui/config.rs:1669` (comment) | "Asteria" |
| Messages and errors | `launch.rs:190,622`, `shares.rs:134,139,252`, `crash.rs:117` (summary header), `import.rs:192` | "Asteria" |
| Comments and docs | `store.rs:1`, `saves.rs:25`, `groups.rs:11`, `widgets.rs:1,64`, `cli.rs:1-9`, `fsutil` and `shares` comments | updated, Omarchy mentions removed |
| README, CONTRIBUTING, AGENTS.md / CLAUDE.md | Omacraft and Omarchy throughout | rewritten (keep credits and licences; replace "Omarchy look" rules with StarShell rules plus an "Approved exceptions" list) |
| Screenshots | `docs/screenshots/*` | retaken after the redesign |
| Desktop entry | `Name=Omacraft`, `Comment=Minecraft launcher themed by Omarchy` | `Name=Asteria`, `Comment=Minecraft launcher` (plus `GenericName=Minecraft Launcher`, `Keywords=minecraft;mods;`) |

**Identity (affects installs, scripts, window rules):**

| Where | Today | Asteria | Note |
|---|---|---|---|
| Cargo package / binary | `omacraft` (`Cargo.toml`) | `asteria` | also `repository = …/asteria` |
| App id / Wayland app-id | `paths::APP_ID = "omacraft"` | `asteria` | **also drives the data and config dirs**, see F.3 |
| Desktop file, icon | `packaging/omacraft.{desktop,svg}`, `StartupWMClass=omacraft` | `asteria.*` | the icon is new artwork (C.1) |
| Install / uninstall | `install.sh` (REPO, binary, paths, `grep '^name = "omacraft"'`), `uninstall.sh` | Asteria names; **also remove an old Omacraft install's binary, desktop file and icon** if present (with a message, never its data) | |
| CLI usage | `omacraft list`… | `asteria …`; **drop `theme`** (no Omarchy theme to print), or have it print the active palette (H.9) | Hyprland binds that call `omacraft launch` need updating by hand |
| User agent | `omacraft/0.1.0 (minecraft launcher)` (`net.rs:10`) | `asteria/<ver> (minecraft launcher)` | Modrinth asks for an identifying UA |
| Game `launcher_name` | `launch.rs:21` `"omacraft"` | `"asteria"` | shows in game crash reports only |
| Env vars | `OMACRAFT_MSA_CLIENT_ID`, test-only `OMACRAFT_FIXTURE`, `OMACRAFT_TEST_FLOCK`, `OMACRAFT_TEST_LOCK`, debug `OMACRAFT_DEBUG_VIEW/KEYS` | `ASTERIA_*`; **read `OMACRAFT_MSA_CLIENT_ID` as a fallback** for one release | test vars just rename |
| Test temp dirs | `omacraft-test-<pid>`, `omacraft-export-…` | `asteria-…` | |

### F.3 Compatibility-sensitive items (user data)

| Item | Where | Risk if renamed blindly | Plan |
|---|---|---|---|
| **Data dir** `~/.local/share/omacraft/` | `paths::data_dir()` via `APP_ID` | instances, worlds, accounts, backups vanish | **First-run migration** (below) |
| **Config** `~/.config/omacraft/config.toml` | `paths::config_dir()` | keys, client id and settings lost | migrated with the data |
| **Absolute paths inside data** | backup snapshot metadata keeps canonical `source` paths (`backups.rs:144`); share links may be absolute (`shares.rs:194`, `is_share_link` accepts the absolute form) | restores and share links point at the old path | a **compatibility symlink** `~/.local/share/omacraft → asteria` keeps them resolving; `shares::apply` already rewrites to the relative form when it runs |
| **Play records** `omacraft.toml` in every world | `world_sets.rs:29` `RECORD` | world-drift checks lose history | **read both** `asteria.toml` and `omacraft.toml` (newest play wins), **write** `asteria.toml`; never delete the old file (worlds may be shared with an Omacraft still in use) |
| **Java runtime marker** `.omacraft-version` | `java.rs:67` | runtimes re-downloaded | read either name, write the new one |
| **Default icon key** `"omacraft"` in `instance.toml` | `instance.rs:365`, `icons.rs:12,22`, `export.rs` (Prism `iconKey`) | instances lose their icon | keep `"omacraft"` as an **alias** of the new default icon in `icons::source`; new instances get `"asteria"`; export maps both to the shipped default |
| **Group colours** in `groups.toml` | `groups.rs` | none if kept | kept as the same names, shown as clarity hues (C.6) |
| **Microsoft client id** | `config::DEFAULT_CLIENT_ID` (Omacraft's Azure app, Mojang-approved) | changing it signs everyone out and needs a new Mojang approval | **keep the same id** (same owner); rename the Azure app's display name in the Azure portal so the consent screen says Asteria (H.7) |
| **Accounts file** `accounts.json` (0600) | data dir | must keep 0600 through migration | moved with the data dir (a rename keeps mode) |
| **Exported packs** | `.mrpack` `summary`, zip `instance.cfg` | none (no launcher name inside) | unchanged |
| **Hyprland drag-and-drop patch** | `vendor/smithay-clipboard` | Hyprland-specific, but not Omarchy | **keep** |

**First-run migration** (proposed for Phase 1):
1. If `~/.local/share/asteria` doesn't exist and `~/.local/share/omacraft` does, and no `omacraft` process is running (`pgrep -x omacraft`), then:
2. Take the existing locks (`shared/.lock`, backup shelf locks) to be sure no job is mid-flight.
3. **Rename** `omacraft → asteria` (same filesystem, atomic; reflinks and permissions intact), then create the symlink `omacraft → asteria`. Do the same for the config dir.
4. Show a notice once: "Moved your instances from Omacraft. Omacraft, if you still run it, now uses the same files." That's honest: both apps share the data through the link and the existing cross-process locks.
5. If Omacraft is running: don't migrate. Start with a strip explaining "Quit Omacraft to move its instances to Asteria" and a `MOVE NOW` button that re-checks.
6. A CLI path for the same, `asteria migrate`, for people who script it.

Alternatives (copy instead of move, or no symlink) are in H.3.

### F.4 Omarchy dependencies and assumptions to remove

| What | Where |
|---|---|
| Omarchy theme reading (`colors.toml`, `shell.toml`, `theme.name`), polling of `~/.local/state/omarchy/current/theme` | `src/theme.rs`, `paths::omarchy_theme_dir()` |
| Hyprland `active-border` as the modal and toast border | `theme.rs` `apply_shell_tokens` |
| Tokyo Night fallback and the test reading `/usr/share/omarchy/themes/tokyo-night` | `theme.rs` `Default`, `fallback_is_omarchy_tokyo_night` |
| fontconfig `monospace` as the UI face ("Omarchy is mono-first"), the `omarchy font set` assumption | `theme.rs` `FontSet` |
| Lowercase UI-label convention, Nerd Font glyph convention, bundled Nerd Font | `AGENTS.md` "Patterns to follow", `assets/fonts/SymbolsNerdFont*` |
| Settings "appearance" section mentioning `omarchy theme set` | `src/ui/settings.rs:18-42` |
| CLI doc "for Hyprland binds and the Omarchy menu", the `theme` command | `src/cli.rs` |
| README "Made for Omarchy", "Omarchy integration" table, desktop Comment | `README.md`, `packaging/omacraft.desktop` |
| "Omarchy windows have no title bar" comment | `src/main.rs` (reword: any compositor) |

Nothing Omarchy-specific is used at runtime beyond those. There are no Omarchy binaries, menus or packages involved.

---

## G. Implementation roadmap

Each phase ends with `cargo build` warning-free, `cargo test` green (plus `--ignored` network tests at phase ends that touch Modrinth), and a review stop. Foundation first, then screens.

### Phase 0: Repository separation

- **Objectives:** run F.1 steps 1–4 (and 5 only with your go-ahead). Commit the brief and plan under `docs/asteria/`.
- **Files:** none in the source. New folder layout.
- **Dependencies:** your approval of this plan and of H.8.
- **Risks:** pushing to the wrong remote. Mitigation: no `origin` until step 5, the push URL to Omacraft disabled, and a `git remote -v` check printed before any push.
- **Validation:** `git log` shows all 71 commits; `git remote -v` shows no Omacraft push URL; `cargo build && cargo test` pass unchanged in the new folder.
- **Visible outcome:** none in the app; an independent repo exists locally (and on GitHub if approved).

### Phase 1: Rename and de-Omarchy (no redesign yet)

- **Objectives:** F.2 cosmetic and identity renames; F.3 compatibility shims (records, Java marker, icon alias, env fallback); the first-run migration; remove the Omarchy theme source and pin the **StarShell light/dark palettes on the existing widgets** as a stop-gap so the app stays usable; update the install and uninstall scripts and the desktop entry; README, AGENTS/CLAUDE rewrite (rules carried over, look rules replaced).
- **Files:** `Cargo.toml`, `src/main.rs`, `paths.rs`, `config.rs`, `cli.rs`, `net.rs`, `launch.rs`, `java.rs`, `world_sets.rs`, `icons.rs`, `instance.rs`, `export.rs`, `theme.rs` (gutted to fixed palettes), `ui/settings.rs`, messages listed in F.2, `packaging/*`, `install.sh`, `uninstall.sh`, docs.
- **Dependencies:** Phase 0; H.3, H.7, H.9.
- **Risks:** data migration (the biggest risk in the whole project); absolute paths; tests that assert `"omacraft"` keys (`export.rs:446-447`).
- **Validation:** new regression tests: `migration_moves_and_links_an_omacraft_dir`, `migration_waits_while_omacraft_runs`, `records_are_read_under_either_name`, `old_icon_key_still_resolves`, `old_client_id_env_is_honoured`. A manual run against a **copy** of the real data (`XDG_DATA_HOME`/`XDG_CONFIG_HOME` scratch, per AGENTS.md): instances, accounts, worlds, backups restore, share sets and a world set all work after migration. Running Omacraft afterwards on the linked dir still works.
- **Visible outcome:** "Asteria" everywhere, cream or dark palette with the old layout, no Omarchy dependence.

### Phase 2: Design foundation

- **Objectives:** E.1 tokens and crossfade, the colour-scheme watcher, reduced-motion read; E.2 fonts and the Phosphor bundle; E.3 kit primitives (rules, captions, buttons, switch, field, slider/meter, setting rows, table, tiles, bar cells, sheet/strip, icon map, motion helpers, `HoverList` with `StarField`); AccessKit info on each.
- **Files:** new `src/design/{tokens,fonts,motion,glyphs}.rs`, `src/design/kit/*`; `theme.rs` removed; `assets/fonts/` (Phosphor in, Nerd Font out).
- **Dependencies:** Phase 1; H.5 (font fallback).
- **Risks:** egui rendering of OTF CFF fonts (Switzer and Melodrama are `.otf`), to check first; animation repaint loops; the D-Bus watcher's availability.
- **Validation:** a hidden **kit gallery** view (`ASTERIA_DEBUG_VIEW=kit`) showing every primitive in both palettes, with screenshots to tune by eye together (the principle "show them running, tune with the user"); idle CPU check (no repaint when still); a run with fonts missing (`FONTCONFIG_FILE` plain-system simulation from AGENTS.md).
- **Visible outcome:** the kit gallery; the old screens are not switched over yet.

### Phase 3: App shell

- **Objectives:** D.1 bars and notices; modals → sheets and strips (rendering only); `Index`/`Instance` view state with the instance header shell; the drop hint.
- **Files:** `ui/mod.rs` (top_bar, status_bar, toasts, show_modal, ui), `ui/nav.rs` (hints as captions, `?` help as a sheet), `ui/instances.rs`.
- **Dependencies:** Phase 2.
- **Risks:** key routing under sheets (`modal_nav`); the escape/back semantics with a new view level; keymap conflicts for new defaults.
- **Validation:** the existing `ui::*` tests; a manual keyboard pass through every dialog with `ASTERIA_DEBUG_KEYS`; a check that panes under a sheet take no keys.
- **Visible outcome:** the new frame of the app, StarShell's bars, dialogs as sheets.

### Phase 4: Index, rail and instance page

- **Objectives:** D.2 Index hover list, compact rail, filter takeover, groups as numbered sections, drag and drop; D.3 header, spec strip, section/sub-tab navigation; D.4 Overview.
- **Files:** `ui/instances.rs`, `ui/version.rs`, notes in `instances.rs`, `ui/group_manager.rs`, `keymap.rs` (only if H.1 needs new actions).
- **Dependencies:** Phase 3; H.1, H.2.
- **Risks:** drag and drop in the hover list (hit areas move with the shift, so hit-test on the unshifted rect); long names; the 760px minimum.
- **Validation:** `ui::instances::tests`; select/play/drag/filter with mouse and keys; narrow and short window sizes; both palettes.
- **Visible outcome:** the signature Asteria look on its main screen.

### Phase 5: Content, mod sets, bisect

- **Objectives:** D.5, D.6.
- **Files:** `ui/content.rs`, `ui/mod_sets.rs`.
- **Dependencies:** Phase 4.
- **Risks:** the dense guard logic (mod jobs, search admission) is interleaved with drawing. Change drawing only, and keep every `ModsBusy` check where it is.
- **Validation:** `ui::content::tests`, `ui::mod_sets::tests`, `mod_sets::tests`, `bisect::tests`; a manual Modrinth install/update/rollback; a bisect start-to-finish on a scratch instance (with the game launched with `WAYLAND_DISPLAY`/`DISPLAY` unset, per AGENTS.md).
- **Visible outcome:** the mods screen and bisect flow redesigned.

### Phase 6: Configs editor

- **Objectives:** D.7.
- **Files:** `ui/config.rs` (the largest file; split view and state here if useful, see E.6).
- **Dependencies:** Phase 2 (setting rows, slider), Phase 4.
- **Risks:** "widgets must not change a value by drawing it" (`SliderClamping::Edits`, `clamp_existing_to_range(false)`): the new slider must honour the same rule.
- **Validation:** `mod_config::tests`, `ui::config::tests`, `vanilla_options::tests`, `shader_config::tests`; a byte-for-byte round-trip check of a real config edited through the new controls.
- **Visible outcome:** the configs editor in settings-row form.

### Phase 7: Worlds, servers, screenshots, logs, crash analysis

- **Objectives:** D.8 to D.10.
- **Files:** `ui/worlds.rs`, `ui/servers.rs`, `ui/screenshots.rs`, `ui/logs.rs`.
- **Dependencies:** Phase 4.
- **Risks:** log-view performance (`show_rows` virtualisation must stay); the image crossfade in screenshots; the world job guards.
- **Validation:** `ui::worlds::tests`, `ui::screenshots::tests`, `crash::tests`, `backups::tests`; a 100k-line log scroll check.
- **Visible outcome:** every per-instance section redesigned.

### Phase 8: Accounts, settings, dialogs

- **Objectives:** D.11 to D.13.
- **Files:** `ui/accounts.rs`, `ui/skin_view.rs` (frame only), `ui/settings.rs`, `ui/new_instance.rs`, `ui/version_picker.rs`, `ui/icon_picker.rs`, `ui/export.rs`, edit in `ui/instances.rs`.
- **Dependencies:** Phase 3.
- **Risks:** the Microsoft sign-in flow and skin upload (network); test with a real account deliberately, once.
- **Validation:** `export::tests::exports_import_back`, an import of each source kind, a sign-in round trip, skin model switch and reset.
- **Visible outcome:** the whole app in the new language.

### Phase 9: Identity and polish

- **Objectives:** the final mark and app icon (H.4), empty and loading states pass, a copy pass (case convention, terminology), accessibility pass (AccessKit labels, focus order, contrast of every text token on its real background), motion tuning session, retaken screenshots, README, version `0.2.0` (H.11) and a release tag.
- **Files:** `packaging/asteria.svg`, `assets/icons/asteria.svg`, docs, `Cargo.toml`.
- **Dependencies:** Phases 4–8.
- **Validation:** a full manual walkthrough in both palettes, at 760×480 and full screen, keyboard only and mouse only.
- **Visible outcome:** a finished Asteria.

---

## H. Decisions (answered 19 November 2026)

| # | Question | Decision |
|---|---|---|
| H.1 | Navigation | **Index plus instance pages, 6 sections** (C.3, D.2–D.3). |
| H.2 | Group colours | **Keep colour, as terracotta clarity hues** used for readability only: groups, errors, warnings, statuses. Never fills or state (C.6). *Changed from the proposal.* |
| H.3 | Data migration | **Move plus a compatibility symlink** (F.3). |
| H.4 | App icon | **Dark background square, rust star with a vertical gradient lightening towards the top** (shiny). Desktop application icon only (C.1). *Changed from the proposal.* |
| H.5 | Missing fonts | **Fall back to system `sans-serif`** with a notice in Settings (E.2). |
| H.6 | Monospace | **JetBrains Mono for logs** and other raw text (D.10, E.2). *Changed from the proposal.* |
| H.7 | Microsoft client id | **Keep Omacraft's Azure app id**; rename the Azure app's display name in the portal. |
| H.8 | Repository | **Public `star-ges/asteria`, branch `main`, a plain repo** (not a GitHub fork). |
| H.9 | CLI `theme` command | **Removed.** |
| H.10 | Notices | **Notices in the bottom bar**; errors stay until dismissed. |
| H.11 | Version | **0.2.0.** |

### Approved exceptions to the StarShell language

Recorded here and, in Phase 1, in Asteria's own `AGENTS.md`, so they aren't "fixed" later.

1. **Clarity hues** (C.6). This breaks "one accent; colour means state". It's justified because launcher content (logs, groups, statuses) becomes an unreadable wall of text without it. The limits: text and small marks only, never fills, washes, borders or state, and every hue at least 4.5:1 on its own background.
2. **JetBrains Mono for raw text** (E.2). This breaks "two typefaces, no monospace". It's justified because logs and stack traces are scanned by column and are easier to read in monospace. The limits: log lines, crash-report quotes and the config text mode only.
3. **A gradient on the desktop icon** (C.1). This breaks "no gradients". It's justified because the icon sits among other apps' icons and needs presence there. The limits: the `.svg` application icon only, never inside the app.

Everything else in this plan is StarShell applied **[S1]** or extended from its principles **[S2]**. The new patterns (spec strip, bottom-bar notices, bracketed warnings, the quiet configs tree, the unsaved-change marker, bisect answer rows, the device-code figure) will be shown in the Phase 2 kit page for review before they spread.

### Still to confirm when we get there

- **Publishing (Phase 0, step 5):** creating the public GitHub repository and pushing is confirmed separately, just before it happens, after you've checked the local result.
