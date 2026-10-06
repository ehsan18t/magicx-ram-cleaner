# UI Design

The GUI redesign: the vocabulary it uses and the decisions behind it. Decisions are settled one at a time and recorded here before any of them is built.

Status: built in four phases (foundations, Overview, Settings with Processes and About, Monitor). Every item in the acceptance checklist below is in the app.

## Glossary

**Memory list**: one of the kernel's physical page lists the app reads from `MemorySnapshot`: In use, Modified, Standby, Free. Together they add up to installed RAM.

**In use**: pages held by running apps and the system (working sets). Only a working-set trim (Aggressive and Nuclear) reclaims them.

**Modified**: pages changed in memory but not yet written to disk. Moderate and above write them out, which turns them into Standby.

**Standby**: cached pages Windows can hand back instantly. Every clean level purges them. Windows refills this list as files are read, which is normal.

**Free**: pages holding nothing (free plus zeroed).

**Memory map**: the segmented bar on the Overview that shows the four memory lists in their fixed colors. It is the app's signature element.

**Clean level**: Gentle, Moderate, Aggressive or Nuclear, as defined in the engine. Each one targets a known set of memory lists.

**Reclaim estimate**: what a clean level is expected to free, computed from the current memory lists before the clean runs.

## Principles

1. Color means a memory list, nothing else. In use, Modified, Standby and Free each have one fixed color, used the same way on every screen. Red is reserved for real memory pressure and for errors, as in Windows itself; the memory map and chart never turn red.
2. One signature element. The memory map is the only bold thing; everything around it stays quiet and native.
3. Say what happens. Every action states what it will do before it runs and what it did afterwards, in plain words.
4. One orchestrated motion. The memory map animates from before to after a clean. There is no other decorative motion.

## Decisions

| # | Decision | Why |
|---|----------|-----|
| D1 | Native Windows 11 look with one signature element: neutral surfaces, Segoe UI, Settings-style rows, and the memory map as the only colorful element. | A Windows system utility reads as production-grade when it looks like it belongs on the OS. |
| D2 | The accent color follows the Windows accent the user picked, updates live when it changes, and falls back to a default blue when it cannot be read. It is used for the primary button, toggle "on" states, the selected navigation item, focus rings and links, never for the memory map. | Matches Windows 11's own apps. A teal Windows accent would share a hue with Standby, but they never appear on the same control, so that is accepted. |
| D3 | The Overview cleans through a level picker and one Clean now button. The picker highlights the memory lists the selected level targets and shows its reclaim estimate. The last picked level is remembered across launches as its own setting, separate from the Monitor's auto-clean level. | One clear primary action instead of four competing ones, and the user sees what a level does before running it. Keeping the two levels separate means a one-off clean never changes what auto-clean does. |
| D4 | Navigation is a collapsible pane in the style of Windows 11 Task Manager. The window opens at about 860 by 600 with a labeled pane (about 170 px). Below about 760 px of width the pane collapses to an icon rail on its own, a menu button toggles it by hand, and the manual choice is remembered. | Labels make the app learnable without tooltips, and this is how Windows 11 utilities behave. The larger default window gives the memory map and process table room. |
| D5 | Theme is System, Light or Dark. System follows the Windows app theme and switches live; it is the default for new settings files. Existing settings files keep their saved Dark or Light choice, converted from the old yes/no value when read. | Following the OS theme is expected of a native app, and an update must never flip a user's theme. |
| D6 | The Processes page gets a Trim action on the hovered row. It trims every instance of that program and reports the result in the row, including how many protected instances were skipped. There is no confirmation, because a trim loses no data. | It makes the page actionable using an engine operation that already exists. |
| D7 | The Monitor's history chart records only when the app already reads memory (window visible or auto-clean on): the last 10 minutes at one point per second. Unrecorded time shows as a gap. | Keeps the existing rule that a tray-hidden app with auto-clean off does no periodic work. |
| D8 | The page sits on a raised layer, as in Task Manager: a slightly lighter surface than the navigation pane and title bar, rounded at the top-left, with a hairline along its top and pane-side edges. | With one shared background the pane and the page read as a single plane, so nothing showed where navigation ends and content begins. |

## Acceptance checklist

Each statement is observable in the running app.

1. The window opens at about 860 by 600 with a labeled pane: Overview, Monitor, Processes and Settings at the top, About at the bottom.
2. Below about 760 px of width the pane collapses to an icon rail. The menu button toggles it, and a manual choice survives a restart.
3. On Windows 11 the title bar has the same color as the app background, in both themes.
4. Text renders in Segoe UI.
5. Accent-colored controls match the Windows accent, and changing the Windows accent updates the app without a restart.
6. The theme setting offers System, Light and Dark. System follows Windows live. A settings file saved by an older version keeps its Dark or Light theme.
7. The Overview shows the memory map: four segments in fixed colors, a legend whose values add up to installed RAM, and a one-sentence summary.
8. The level picker shows the four levels. Selecting one dims the memory lists it does not target and shows its reclaim estimate and a plain description. The selection survives a restart and does not change the auto-clean level.
9. Clean now runs the selected level and shows progress while it runs. Afterwards the memory map animates to the new state and the amount freed and the time taken are shown.
10. When the kernel page lists cannot be read, the memory map shows In use and Available only, and the picker says no estimate is available.
11. Red appears only for memory pressure (a load of 90% or higher) and for errors. The memory map and chart never turn red.
12. The Monitor shows a 10-minute history chart in the memory-list colors with the auto-clean threshold drawn as a line, gaps where nothing was recorded, and an event list that shows what each auto-clean freed.
13. The Processes page has a Top 10 / 20 / 50 selector, a search box with a built-in clear button, a highlighted row on hover, and a Trim button on the hovered row that reports the amount freed and any skipped instances. Rows also take keyboard focus, and Enter or Space trims the focused row.
14. The Settings page uses rows with an icon, title, description and a control on the right. Toggle switches show on and off clearly. The context menu is one row with its status and one button.
15. Hidden in the tray with auto-clean off, the app does no periodic work, as before.
16. Text in both themes meets WCAG AA contrast, and keyboard focus is always visible.

## Assumptions

- No Mica backdrop: only the title bar color changes. Windows 10 keeps today's dark or light title bar.
- The tray icon and tray menu stay as they are.
- The About page keeps its content and is restyled with the new colors and type.
- No confirmation before any clean level, as today. The level description states the side effects.
- Only the pane state is remembered; window size and position are not.
- Dashboard is renamed Overview.
- Aggressive and Nuclear estimates show the predictable part (Modified and Standby) and say that part of app memory comes on top, since a working-set trim cannot be predicted exactly.
- Segoe UI is loaded from `C:\Windows\Fonts` at runtime and never shipped. If it is missing, egui's built-in font is used.
- The work lands in four phases, each with its own commits and before and after screenshots: foundations (colors, type, title bar, pane, theme setting), Overview, Settings and Processes, then Monitor.
