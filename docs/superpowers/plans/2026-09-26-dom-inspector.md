# The DOM inspector — the browser's F12, over the Chrome DevTools Protocol

**Written 2026-09-26** from the tree at `aa8ed9d16`, after reading `CLAUDE.md`
(RULE 0 rows for `rts-dom`, `rts-dom-bridge`), `crates/rts-dom/PLAN.md` §0–§2,
`docs/ui/html-engine/box-tree.md` §7, `docs/engine/lost-roots.md`,
`crates/rts-core/README.md` rule 10, and the code named below. It is the plan
an agent executes; the rules are the crate READMEs and `CLAUDE.md`.

The goal: the REAL Chrome DevTools frontend (`chrome://inspect`, or the
`devtools://` URL this prints) attaches to a running `rts` program and shows
its document in the Elements panel — tree, computed style, matched rules with
origin and specificity, box model, hover highlight — with no cost at all to a
program that did not ask for it.

## What is true today, measured

- **A CDP-shaped method dispatch exists, in process only.**
  `crates/rts-node/src/inspector/session.rs:41` (`dispatch`) answers
  `Runtime.enable/disable`, `Runtime.evaluate` (through `entry::evaluate`, the
  seam `node:vm` uses), `Runtime.getHeapUsage`, `Schema.getDomains`, and refuses
  `Profiler.*` by name. It is reached from `session.post` (`session.rs:128`)
  and builds its answers as runtime objects.
- **A discovery endpoint exists, and stops at the upgrade.**
  `crates/rts-node/src/inspector/endpoint.rs:59` (`open`) binds
  `127.0.0.1:<port>`, spawns one `rts-inspector` accept thread, and answers
  `/json`, `/json/list`, `/json/version` (`respond`, line 115). Its own doc
  (lines 11–16) and `mod.rs:30–34` name the missing piece: **no WebSocket
  upgrade and no JSON-RPC command loop**, so a frontend probes and then fails
  to attach. `/json/list` answers `"type":"node"`, which DevTools opens without
  an Elements panel.
- **The `session.rs:146` panic named in the brief is already fixed on this
  base**: commit `1d12e6cb1` replaced `entry::null_value()` inside the
  `with_runtime` closure of the `Evaluate` arm with `entry::null_in(context)`
  (line 155 now; line 146 is the comment explaining it). The pinning test is
  `crates/rts-host/tests/node_modules.rs:1190`
  (`the_inspector_backs_what_it_can_and_refuses_the_rest_by_name`), whose
  `Runtime.evaluate answers the value` case is the one that aborted.
- **A WebSocket server core exists and is reusable as is.**
  `crates/rts-node/src/ws/handshake.rs:54` (`read_request`) and `:104`
  (`response`) are the server side of RFC 6455's opening handshake;
  `ws/frame.rs:82` (`read_frame`) and the encoder beside it are the framing,
  written "knowing neither client nor server, only the RFC" (`ws/mod.rs:17`).
  `import { WebSocketServer } from "ws"` already runs on them. `reuse-check`
  answer: the inspector's socket is these two modules, not a third copy.
- **The document is reachable from Rust by handle.**
  `crates/rts-dom/src/store.rs:17` keeps `handle → Dom` in a `thread_local` of
  the program's thread; `with_dom`/`with_dom_mut` (lines 46, 51) lend it. There
  is no way to list the live handles.
- **Nodes already have versioned identity.** `NodeId { generation, idx }`
  (`crates/rts-dom/src/dom/no.rs:15`), resolved by `Dom::resolve`
  (`dom/arvore.rs:115`). It holds no runtime value — plain Rust data in
  `rts-dom`'s own arena — so a table of them is not a GC root question
  (`lost-roots.md` is about tables of HANDLES into the runtime region).
- **Everything the domains answer already exists as a query**:
  `Dom::computed_property` (`dom/estilo.rs:116`, the `getComputedStyle`
  answer), `Dom::inline_property`/`css_text` (`:246`, `:261`),
  `Dom::bounding_component` (`dom/geometria.rs:44`) over `layout_cached` +
  `geometry_cached`, `DisplayList::rect_of_in` (`query/rect.rs:43`),
  `DisplayList::hit_test` (`query/hit.rs:30`), and
  `Stylesheet::matched_for_node` (`style/stylesheet/sheet.rs:389`) which answers
  the matched rules in cascade order with origin (`is_ua`) and specificity.
- **What does not exist**: a textual form of a parsed selector (`Rule` keeps
  the `ComplexSelector` only, `style/stylesheet/mod.rs:42`), and any "DOM
  config object" — `rts:dom` is a flat namespace of primitives
  (`crates/rts-dom-bridge/src/lib.rs:62`), and no member takes an options bag.
- **The frame loop pumps loop sources.** A windowed page calls
  `engine.runEventLoop` per frame (`rts-dom-bridge/src/engine.rs:82`), which
  calls `entry::pump_sources`; headless, the host loop does
  (`crates/rts-host/src/run.rs:460`). So a source registered with
  `entry::declare_loop_source` is served in both modes without new plumbing.
- **The window paints in one place**: `render_dom_scrolled`
  (`crates/rts-egui/src/frame/render/mod.rs:244`), with the content origin
  (`ui.max_rect().min`) and page offset in hand at line 360.

## The FORM, fixed first

**F1. Off by default, three ways on, one switch.** The switch is
`rts_node::inspector::cdp::request(port)`. It is pulled by: the CLI flag
`--inspect[=port]` (Node's spelling, default port 9229); the environment
variable `RTS_INSPECT` (`1`/empty → 9229, a number → that port); and the
`rts:dom` config call `configure({ inspector: true | { port } })`, which is the
config object this plan introduces because none existed. All three end in the
same `cdp::start(port)`.

**F2. Off means nothing exists.** No port is bound, no thread is spawned, no
loop source is registered, no per-node table is allocated, and the domain code
is not compiled into a build that does not ask for it: the CDP transport is
behind `rts-node`'s feature `cdp`, the domains behind `rts-dom-bridge`'s feature
`inspector`, and `rts-host`'s feature `inspector` (default ON for the CLI)
turns both on. `rts-runtime-boot` — the AOT `.exe` — does not enable it. The
one residual cost with the feature compiled in and the switch off is one
thread-local read per painted frame (the overlay asks whether a node is
highlighted); it is per frame, never per node.

**F3. On means a real CDP endpoint, localhost only.** The existing endpoint
grows the WebSocket upgrade on `/<id>` (through `ws/handshake.rs` and
`ws/frame.rs`); `/json/list` answers `"type":"page"` with a
`devtoolsFrontendUrl` once DOM domains are registered, so `chrome://inspect`
lists it under "Remote Target" and opens the Elements panel. The bind stays
`127.0.0.1` (the endpoint doc's reason stands). Starting prints
`Debugger listening on ws://…` and the `devtools://devtools/bundled/inspector.html?ws=…`
URL to stderr, as Node does.

**F4. One dispatch, two transports.** The socket does not get its own
JSON-RPC interpreter: a message is parsed on the socket thread, queued, and
executed ON THE PROGRAM'S THREAD by a loop source — the only thread that may
touch the document (`store.rs` is a `thread_local`) or the runtime. There,
`Runtime.*`/`Schema.*` go to `session.rs`'s existing `dispatch` (made
`pub(super)`, its answer serialised with `entry::json_stringify`), and
`DOM.*`/`CSS.*`/`Overlay.*` go to a domain handler that `rts-host` wires in
with `cdp::declare_domains`. `rts-node` never learns `rts-dom`; `rts-host` is
the crate that may name both (CLAUDE.md, "the engine in one paragraph").

**F5. Node ids: a side table per session, of `(handle, NodeId)`.** CDP wants a
small integer per node, stable for the life of the document. The table maps
`u32 → (document handle, NodeId)` and back, grows only when a node is SENT to
the frontend, and lives as long as the program's thread — so a frontend that
reconnects sees the same ids — and is never allocated when nothing attaches.
It holds no runtime
handle, so rule 10 of `rts-core` (every table of handles is a root list) does
not apply — stated here because it is the first question a reviewer will ask.
A stale entry fails `Dom::resolve` by generation and answers "no node".
`backendNodeId` is the same number.

**F6. Which document.** The inspector shows the most recently created live
document (`store::latest()`, new). A page opened in a window is that document.
Listing every document as its own target is phase 2.

**F7. The overlay is state in `rts-dom`, paint in `rts-egui`.**
`rts_dom::inspect::set_highlight(handle, Some(NodeId))` stores one
`(handle, NodeId)` in a thread-local cell; `render_dom_scrolled` asks for it
and paints the content/padding/border/margin quads with DevTools' colours on
top of the page. Headless there is nothing to paint and the call is a store.

**F8. Whitespace-only text nodes are not shown**, as Chrome's Elements panel
does not show them. They still exist in the document; the projection skips
them and `childNodeCount` counts what is shown.

### Phase 1 (this lot)

- config + CLI + env (F1), the endpoint and upgrade (F3), the socket transport
  and loop source (F4);
- `DOM.enable/disable`, `DOM.getDocument`, `DOM.requestChildNodes` (emits
  `DOM.setChildNodes`), `DOM.describeNode`, `DOM.getBoxModel`,
  `DOM.getNodeForLocation`, `DOM.pushNodesByBackendIdsToFrontend`;
- `CSS.enable/disable`, `CSS.getComputedStyleForNode`,
  `CSS.getMatchedStylesForNode` (origin `regular`/`user-agent`, specificity
  `{a,b,c}`, declarations with `important`), `CSS.getInlineStylesForNode`;
- `Overlay.enable/disable`, `Overlay.highlightNode`, `Overlay.hideHighlight`;
- `Page.enable`, `Page.getResourceTree` (one frame), and acknowledgements of
  the enable calls DevTools sends unconditionally; every other method answers
  a JSON-RPC error `-32601` by name — never a fabricated result.
- `Runtime.evaluate` from the console, via the existing session dispatch.

### Phase 2 (named, not in this lot)

- the write set: `DOM.setAttributeValue`, `DOM.removeNode`, `DOM.setOuterHTML`,
  `CSS.setStyleTexts` (live edits), and the mutation events
  (`DOM.childNodeInserted/Removed`, `attributeModified`) they need;
- `Overlay.setInspectMode` — the picker: it needs the window's click to be
  captured before it reaches the page, which is an input-path change in
  `rts-egui`;
- one target per live document; `Runtime.executionContextCreated`,
  `consoleAPICalled` (console messages streamed to the frontend);
- later: `Debugger` breakpoints on DOM events, `Network`, `Performance`.

## Rulers

- `cargo check` of `rts-node`, `rts-dom-bridge`, `rts-host`, `rts-egui`,
  `rts-cli` with and without the features.
- Unit tests, protocol shape: `DOM.getDocument` over a fixture page answers the
  expected `nodeId`/`nodeName`/`children`; `CSS.getComputedStyleForNode`
  answers the same string `Dom::computed_property` (the `getComputedStyle`
  seam) answers; `CSS.getMatchedStylesForNode` lists an author rule with its
  selector text and specificity; a selector round-trips through its text.
- `crates/rts-host/tests/node_modules.rs`'s inspector test passes.
- Manual proof: a headless page run with `--inspect`, a raw WebSocket client
  prints the `DOM.getDocument` answer.
- Grep proof for F2: nothing reachable from `start_if_requested` binds or
  spawns when the switch is off.

## Tasks

1. `rts-dom`: `store::latest`, `inspect` module (highlight cell), selector
   serialisation (`style/selector/serialize.rs`), `Dom::matched_rules_view`
   (`dom/inspect.rs`).
2. `rts-node` feature `cdp`: `inspector/cdp/` (switch, transport, loop source,
   domain registry); endpoint upgrade and `type:page`.
3. `rts-dom-bridge` feature `inspector`: `src/inspector/` (node table,
   `DOM`, `CSS`, `Overlay`), `configure` member.
4. `rts-host` feature `inspector`: wire the domains and the switch; `rts-cli`
   `--inspect[=port]`.
5. `rts-egui`: `frame/render/overlay.rs`.
6. Tests and the manual proof.

## Constraints

- Files ≤ 500 lines (every crate touched is outside the two engine crates).
- No thread, port, source or table when off (F2); no second JSON-RPC loop (F4);
  no second WebSocket framing (reuse `ws/`).
- A method that is not implemented refuses by name. An empty answer that looks
  like a working one is the failure this repository refuses.
- Code, identifiers and comments in English.
