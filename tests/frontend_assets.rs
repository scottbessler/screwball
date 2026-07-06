const GAME_JS: &str = include_str!("../public/game.js");
const APP_CSS: &str = include_str!("../public/app.css");
const SW_JS: &str = include_str!("../public/sw.js");
const NOTIFICATION_DEBUG_JS: &str = include_str!("../public/notification-debug.js");
const TOUCH_DEBUG_JS: &str = include_str!("../public/touch-debug.js");

#[test]
fn htm_native_tags_do_not_split_immediately_after_tag_name() {
    let native_tags = [
        "a", "aside", "button", "div", "form", "h1", "h2", "li", "ol", "p", "span", "table",
    ];
    for (line_index, line) in GAME_JS.lines().enumerate() {
        let trimmed = line.trim_end();
        for tag in native_tags {
            assert!(
                !trimmed.ends_with(&format!("html`<{tag}")),
                "native <{tag}> template starts on one line and continues on the next at public/game.js:{}",
                line_index + 1,
            );
        }
    }
}

#[test]
fn pending_tiles_can_be_recalled_by_dropping_on_rack() {
    assert!(
        GAME_JS.contains("e.dataTransfer.setData(\"text/plain\", \"pending:\" + pending.rackId);"),
        "pending board tiles must advertise a pending:<rackId> drag payload",
    );
    assert!(
        GAME_JS.contains("function pendingRackIdFromDrop(e)"),
        "rack drops need to parse pending tile payloads",
    );
    assert!(
        GAME_JS.contains("function handleRackDrop(e)")
            && GAME_JS.contains("onDrop=${handleRackDrop}")
            && GAME_JS.contains("onRecallPending(rackId);"),
        "dropping a pending tile on the rack must recall it",
    );
    assert!(
        GAME_JS.contains("if (pendingRackId != null && onRecallPending)"),
        "dropping on an individual rack tile must recall pending tiles before rack reorder cleanup",
    );
    assert!(
        GAME_JS.contains("if (pendingDrag.current) handleBoardTouchEnd(e);")
            && GAME_JS.contains("document.addEventListener(\"touchend\", onDocumentTouchEnd"),
        "touch recall must survive ending the drag over the rack, outside the board element",
    );
    assert!(
        GAME_JS.matches("onRecallPending=${recallByRackId}").count() >= 2,
        "both board and rack components should receive pending recall callbacks",
    );
    assert!(
        !GAME_JS.contains("onRecallPending={"),
        "htm attributes must use onRecallPending=${{...}}, not JSX-style braces",
    );
}

#[test]
fn app_layout_wrappers_use_plain_native_divs() {
    assert!(
        !GAME_JS.contains("NativeDiv"),
        "layout wrappers should not use a dynamic component for plain divs",
    );
    assert!(
        GAME_JS.contains("return h(\"div\", { class: \"game\" }, ["),
        "App root wrapper should be built with h() so htm cannot emit literal div text",
    );
    assert!(
        GAME_JS.contains("h(\"div\", { class: \"game-layout\" }, [")
            && GAME_JS.contains("h(\"div\", { class: \"play-column\" }, [")
            && GAME_JS.contains("h(\"div\", { class: \"board-wrap\" }, [")
            && GAME_JS.contains("h(\"div\", { class: \"rack-area\" }, ["),
        "layout wrappers should be explicit h() divs instead of top-level htm div templates",
    );
}

#[test]
fn game_script_avoids_webkit_fragile_results_syntax() {
    assert!(
        !GAME_JS.contains("toSorted("),
        "game script should avoid toSorted because older WebKit builds fail at runtime",
    );
    assert!(
        !GAME_JS.contains("html`<li key=${seat.index}"),
        "finished-game results should avoid multiline htm attributes around seat.index",
    );
    assert!(
        GAME_JS.contains("key: seat.index")
            && GAME_JS.contains("const ranked = [];")
            && GAME_JS.contains("ranked.splice(insertAt, 0, seat);"),
        "results rendering should use explicit h() markup and broadly supported ranking",
    );
}

#[test]
fn rack_recall_and_tile_text_styles_are_present() {
    assert!(
        css_rule_contains(
            ".rack.rack-recall-over",
            "outline: 2px solid var(--accent);"
        ),
        "rack should visibly advertise itself as the pending-tile recall target",
    );
    assert!(
        css_rule_contains(".rack-tile", "color: var(--tile-ink);"),
        "rack tile buttons should override mobile default button/link text color",
    );
    assert!(
        css_rule_contains(".rack", "width: 100%;")
            && css_rule_contains(".rack-area", "width: min(100%, 600px);")
            && APP_CSS.contains(
                ".rack-area {\n    margin-top: 0.4rem;\n    flex-shrink: 0;\n    width: 100%;"
            ),
        "rack width should stay stable regardless of John Mode helper content",
    );
}

#[test]
fn board_labels_are_large_enough_on_mobile() {
    assert!(
        css_rule_contains(
            ".cell .tile-letter",
            "font-size: clamp(0.78rem, 2.35vw, 1.12rem);"
        ) && css_rule_contains(
            ".premium-label",
            "font-size: clamp(0.44rem, 1.3vw, 0.62rem);"
        ) && css_rule_contains(".premium-label", "letter-spacing: 0;")
            && APP_CSS
                .contains(".premium-label {\n    font-size: clamp(0.46rem, 2.15vw, 0.62rem);"),
        "board tile and premium-square labels should be readable without overwhelming the squares",
    );
    assert!(
        APP_CSS.contains("--board-bg: #e3dece;")
            && APP_CSS.contains("--tile: #fff1b8;")
            && APP_CSS.contains("--tile-edge: #d2b85f;")
            && css_rule_contains(".cell", "border-radius: 2px;"),
        "tile bodies should contrast against the board and use tighter square rounding",
    );
}

#[test]
fn john_mode_hint_is_stable_while_crossing_rack_gaps() {
    assert_eq!(
        GAME_JS
            .matches("onMouseLeave=${() => onHover && onHover(null)}")
            .count(),
        1,
        "John Mode hover should clear only when leaving the whole rack, not each tile gap",
    );
    assert!(
        GAME_JS.contains("onFocus=${() => onHover && onHover(tile.is_blank ? null : tile.letter)}")
            && GAME_JS.contains("onBlur=${() => onHover && onHover(null)}"),
        "John Mode rack hints should also react to keyboard focus",
    );
    for declaration in [
        "position: absolute;",
        "top: calc(3rem + 0.35rem);",
        "opacity: 0;",
        "transform: translateY(-0.3rem);",
        "padding: 0.45rem 0.55rem 0.65rem;",
        "overflow: visible;",
        "visibility: hidden;",
        "transition: opacity 0.16s ease-in-out, transform 0.16s ease-in-out, visibility 0.16s ease-in-out;",
    ] {
        assert!(
            css_rule_contains(".john-tooltip", declaration),
            "John Mode hint should be an animated overlay tooltip: {declaration}",
        );
    }
    assert!(
        css_rule_contains(".john-tooltip.is-visible", "opacity: 1;")
            && css_rule_contains(".john-tooltip.is-visible", "transform: translateY(0);")
            && css_rule_contains(".john-tooltip.is-visible", "visibility: visible;")
            && GAME_JS.contains("class: `john-tooltip${johnLetter ? \" is-visible\" : \"\"}`")
            && !GAME_JS.contains("rack-info-scroll")
            && !APP_CSS.contains(".rack-info-scroll")
            && !css_rule_contains(".john-tooltip", "bottom: calc(100% + 0.35rem);"),
        "John Mode tooltip should toggle below the rack without reflowing the page or covering the board",
    );
    assert!(
        !GAME_JS.contains("john-tooltip-body")
            && !APP_CSS.contains(".john-tooltip-body")
            && !css_rule_contains(".john-tooltip", "max-height:")
            && !css_rule_contains(".john-tooltip", "overflow-y: auto;")
            && css_rule_contains(".john-tooltip", "overflow: visible;"),
        "John Mode tooltip should grow to fit all words instead of clipping inside a max-height scroller",
    );
    assert!(
        css_rule_contains(".john-hint", "flex-wrap: wrap;")
            && css_rule_contains(".john-hint", "min-height: 1.6em;")
            && css_rule_contains(".john-hint", "white-space: normal;")
            && css_rule_contains(".john-hint", "margin: 0;")
            && !css_rule_contains(".john-hint", "min-height: calc(1.6em * 7);")
            && !css_rule_contains(".john-hint", "overflow-x: auto;")
            && !css_rule_contains(".john-hint", "white-space: nowrap;"),
        "John Mode hint should wrap inside its scroll area without reserving a giant blank region",
    );
    assert!(
        css_rule_contains(".john-hint-label", "flex: 0 0 100%;")
            && css_rule_contains(".john-word", "flex: 0 0 auto;")
            && css_rule_contains(".john-word", "letter-spacing: 0;"),
        "John Mode word chips should not force helper reflow",
    );
    assert!(
        GAME_JS.contains("const GRANDPA_TWO_LETTER_WORDS")
            && GAME_JS.contains("document.getElementById(\"grandpa-two-letter-words\")")
            && GAME_JS.contains("(!grandpaMode || GRANDPA_TWO_LETTER_WORDS.has(w))")
            && GAME_JS.contains("grandpaMode=${game.grandpa_mode}"),
        "John Mode helper should honor Grandpa Mode's 2-letter allowlist",
    );
}

#[test]
fn board_drag_preview_shows_exact_landing_tile() {
    assert!(
        GAME_JS.contains("function showBoardDropGhost(cell, preview = activeDragPreview)")
            && GAME_JS.contains("cell.classList.add(\"drag-over\", \"board-drop-ghost\");")
            && GAME_JS.contains("cell.dataset.dropLetter = preview.letter;")
            && GAME_JS.contains("cell.dataset.dropPoints = preview.points;"),
        "board drag should render a visible ghost tile in the candidate destination cell",
    );
    assert!(
        GAME_JS.contains("currentBoardCell: null")
            && GAME_JS.contains("pd.currentBoardCell = cell;")
            && GAME_JS.contains("touchState.current.currentBoardCell = dropTarget.cell;")
            && GAME_JS.contains("const previewCell = pd.currentBoardCell;")
            && GAME_JS.contains("const boardCell = state.currentBoardCell ||"),
        "touch drop should commit to the currently previewed board cell when one is visible",
    );
    assert!(
        GAME_JS.contains("setActiveDragPreview(")
            && GAME_JS.contains("clearActiveDragPreview()")
            && css_rule_contains(".cell.board-drop-ghost", "opacity: 1;")
            && css_rule_contains(
                ".cell.board-drop-ghost::before",
                "content: attr(data-drop-letter);"
            )
            && css_rule_contains(
                ".cell.board-drop-ghost::after",
                "content: attr(data-drop-points);"
            ),
        "mouse and touch drag previews should share the board ghost tile styling",
    );
}

#[test]
fn mobile_touch_drop_target_is_separate_from_dragged_tile() {
    assert!(
        GAME_JS.contains("function boardDropLiftPx()")
            && GAME_JS.contains("return dragGhostLiftPx() * 0.4;")
            && GAME_JS.matches("boardDropLiftPx()").count() >= 3,
        "mobile touch drop target should sit 60% closer to the finger than the lifted tile ghost",
    );
    assert!(
        TOUCH_DEBUG_JS.contains("dropX")
            && TOUCH_DEBUG_JS.contains("dropY")
            && TOUCH_DEBUG_JS.contains("tileX")
            && TOUCH_DEBUG_JS.contains("tileY")
            && TOUCH_DEBUG_JS.contains("Math.round(-lift * 0.4)")
            && TOUCH_DEBUG_JS.contains("oldTargetOffsets"),
        "touch debug page should expose independent drop-point and dragged-tile offsets",
    );
    assert!(
        css_rule_contains(
            ".touch-debug-slider-row",
            "grid-template-columns: 3.25rem minmax(0, 1fr) 3.25rem;",
        ) && css_rule_contains(
            ".rack-tile-ghost.touch-debug-ghost",
            "translate(-50%, -50%)",
        ) && css_rule_contains(".touch-debug-marker", "position: fixed;"),
        "touch debug controls and markers should be usable on mobile while dragging",
    );
}

#[test]
fn last_play_highlight_sits_outside_tile_content() {
    assert!(
        GAME_JS.contains("const cls = lastPlay ? \"cell tile last-play\" : \"cell tile\"")
            && !GAME_JS.contains("lastPlay=${lp}")
            && !APP_CSS.contains(".cell .tile-face.last-play"),
        "last-play class should be on the board cell, not inside the tile face",
    );
    assert!(
        css_rule_contains(".cell.tile.last-play", "overflow: visible;")
            && css_rule_contains(".cell.tile.last-play::after", "inset: -3px;")
            && css_rule_contains(
                ".cell.tile.last-play::after",
                "border: 2px solid var(--accent);"
            )
            && css_rule_contains(".cell.tile.last-play::after", "border-radius: 5px;")
            && css_rule_contains(".cell.tile.last-play::after", "pointer-events: none;"),
        "last-play highlight should render as an outside ring matching the tile radius",
    );
}

#[test]
fn mobile_game_controls_are_compact_and_score_is_separate() {
    assert!(
        GAME_JS.contains("Play ${pendingScore != null ? pendingScore : \"\"}")
            && GAME_JS.contains("}, \"Clear\")")
            && GAME_JS.contains("}, \"Swap\")")
            && GAME_JS.contains("}, \"Next\")")
            && GAME_JS.contains("class: \"button play-button\"")
            && !GAME_JS.contains("Play word")
            && !GAME_JS.contains("Exchange…")
            && !GAME_JS.contains("Confirm exchange"),
        "mobile controls should use state-specific compact labels",
    );
    assert!(
        GAME_JS.contains("function goToNextGame({ activeOnly = false } = {})")
            && GAME_JS.contains("fetch(\"/api/my-games\")")
            && GAME_JS.contains("g.status !== \"finished\"")
            && GAME_JS.contains("window.location.href = `/games/${candidates[0].id}`")
            && GAME_JS.contains("window.confirm(\"Pass your turn?\")"),
        "controls should support next-game navigation and confirm pass",
    );
    assert!(
        GAME_JS.contains("header-pending-score")
            && GAME_JS.contains("pending.textContent = `+${pendingScore}`")
            && !GAME_JS.contains("class: \"pending-score\""),
        "pending word score should render in the header, not above the board or inside the Play button",
    );
    assert!(
        GAME_JS.contains("rack-backspace-tile")
            && GAME_JS.contains("aria-label=\"Backspace\"")
            && GAME_JS.contains("showBackspace=${mode === \"place\" && pending.length > 0}")
            && css_rule_contains(".rack-backspace-tile", "display: none;"),
        "mobile typing should get a rack-shaped backspace control hidden by default",
    );
    assert!(
        css_rule_contains(
            "@media (max-width: 480px) {\n  .rack-backspace-tile",
            "display: flex;"
        ) || APP_CSS.contains("@media (max-width: 480px)")
            && APP_CSS.contains(".rack-backspace-tile {\n    display: flex;"),
        "backspace tile should be visible in the mobile layout",
    );
    assert!(
        css_rule_contains(".controls", "display: flex;")
            && APP_CSS.contains("flex-wrap: nowrap;")
            && APP_CSS.contains("font-size: 0.78rem;")
            && css_rule_contains(".controls .button", "min-width: 7rem;")
            && css_rule_contains(".controls .play-button", "min-width: 7rem;"),
        "mobile controls should be sized to stay on one line without shifting between states",
    );
}

#[test]
fn desktop_game_layout_keeps_actions_near_board_without_zoom_controls() {
    assert!(
        !GAME_JS.contains("boardScale")
            && !GAME_JS.contains("boardWidth")
            && !GAME_JS.contains("board-zoom-controls")
            && !GAME_JS.contains("board-scale-frame")
            && !GAME_JS.contains("Increase board size")
            && !GAME_JS.contains("Decrease board size")
            && !APP_CSS.contains("board-zoom-controls")
            && !APP_CSS.contains("board-scale-frame"),
        "board resizing controls and wrappers should not be present",
    );
    assert!(
        css_rule_contains(".play-column", "display: flex;")
            && css_rule_contains(".play-column", "flex-direction: column;")
            && css_rule_contains(".play-column", "gap: 1rem;")
            && css_rule_contains(".board-wrap", "width: min(100%, 600px);")
            && css_rule_contains(".rack-area", "width: min(100%, 600px);")
            && !css_rule_contains(".sidebar", "grid-row: 1 / span 2;"),
        "desktop rack/actions should stay directly under the board in an independent play column",
    );
    assert!(
        css_rule_contains(
            ".game-layout",
            "grid-template-columns: minmax(min(360px, 100%), 1fr) minmax(min(200px, 100%), 1fr);"
        ) && !APP_CSS.contains("max-width: min(1440px, calc(100vw - 2rem));"),
        "game page should keep the original flexible board/sidebar sizing",
    );
}

#[test]
fn desktop_game_page_keeps_scroll_inside_game_surfaces() {
    assert!(
        APP_CSS.contains("@media (min-width: 481px)")
            && css_rule_contains("body.game-page", "overflow: hidden;")
            && css_rule_contains(".game-page .page", "height: calc(100vh - 3.1rem);")
            && css_rule_contains(".game-page .page", "overflow: visible;")
            && css_rule_contains(".game-page .page > .card:last-child", "height: 100%;")
            && css_rule_contains(".game-page .page > .card:last-child", "display: flex;")
            && css_rule_contains(".game-page .page > .card:last-child", "overflow: visible;")
            && css_rule_contains(".game-page .game", "height: 100%;")
            && css_rule_contains(".game-page .game", "display: flex;")
            && css_rule_contains(".game-page .game-layout", "flex: 1 1 auto;")
            && css_rule_contains(".game-page .game-layout", "min-height: 0;")
            && css_rule_contains(".game-page .sidebar", "align-self: stretch;")
            && css_rule_contains(".game-page .sidebar", "min-height: 0;")
            && css_rule_contains(".game-page .sidebar", "overflow-y: auto;"),
        "desktop game detail pages should not grow the document scrollbar",
    );
    assert!(
        css_rule_contains(".rack-area", "position: relative;")
            && css_rule_contains(".john-tooltip", "position: absolute;")
            && css_rule_contains(".john-tooltip", "top: calc(3rem + 0.35rem);")
            && !css_rule_contains(".john-tooltip", "top: calc(100% + 0.35rem);"),
        "John Mode tooltip should overlay from the rack area instead of extending page height",
    );
}

#[test]
fn other_games_sidebar_hides_finished_games() {
    assert!(
        GAME_JS.contains("data.filter((g) => g.id !== gameId && g.status !== \"finished\")")
            && GAME_JS
                .contains("games.filter((g) => g.id !== gameId && g.status !== \"finished\")"),
        "the in-game other-games sidebar should not render finished games or notify for them",
    );
}

#[test]
fn dark_mode_uses_browser_preference_and_keeps_tiles_legible() {
    assert!(
        APP_CSS.contains("@media (prefers-color-scheme: dark)")
            && APP_CSS.contains("--tile-ink: #1f2933;"),
        "dark mode should follow the browser preference and keep a dedicated tile text color",
    );
    assert!(
        css_rule_contains(".cell.tile", "color: var(--tile-ink);")
            && css_rule_contains(".rack-tile", "color: var(--tile-ink);")
            && css_rule_contains(".cell .tile-face.pending", "color: var(--tile-ink);")
            && css_rule_contains(".cell.board-drop-ghost::before", "color: var(--tile-ink);"),
        "light tile faces should not inherit the dark page text color",
    );
}

#[test]
fn header_has_single_home_link_and_mobile_score() {
    assert!(
        GAME_JS.contains("function renderHeaderScores(game, pendingScore)")
            && GAME_JS.contains("header-score")
            && GAME_JS.contains("turn-dot")
            && GAME_JS.contains("item.classList.add(\"on-turn\")")
            && GAME_JS.contains("return name;")
            && !GAME_JS.contains("if (seat.is_you) return \"you\""),
        "mobile header should render player names/scores and mark the current turn by treatment",
    );
    assert!(
        !GAME_JS.contains("demo-link"),
        "demo link should not exist in the client bundle",
    );
    assert!(
        APP_CSS.contains(".nav .brand {\n  justify-self: start;\n  display: inline-flex;\n  width: 1.5rem;\n  height: 1.5rem;")
            && APP_CSS.contains(".nav .brand {\n    width: 1.15rem;\n    height: 1.15rem;")
            && APP_CSS.contains("height: calc(100dvh - 41px);"),
        "header icon should stay compact and preserve the previous mobile header height",
    );
    assert!(
        css_rule_contains(".header-score-name", "overflow: hidden;")
            && css_rule_contains(".header-score-name", "text-overflow: ellipsis;")
            && css_rule_contains(".header-score-name", "white-space: nowrap;")
            && css_rule_contains(".header-score-value", "flex: 0 0 auto;")
            && css_rule_contains(".header-score", "flex: 0 1 auto;")
            && css_rule_contains(".header-score", "max-width: min(10rem, 42vw);")
            && APP_CSS.contains(
                ".header-score,\n.header-pending-score {\n  display: inline-flex;\n  align-items: center;\n  justify-content: center;"
            )
            && APP_CSS.contains(".header-score {\n    max-width: min(9rem, 40vw);"),
        "long player names should ellipsize while centered score pills stay content-sized with max-widths",
    );
}

#[test]
fn hint_ui_has_no_unlimited_affordance() {
    assert!(
        !GAME_JS.contains("hints_unlimited")
            && !GAME_JS.contains("Hint (∞)")
            && !GAME_JS.contains("unlimited hints"),
        "hints should always be limited; Jax Mode only allows proper names",
    );
    assert!(
        GAME_JS.contains("seat.hints_remaining") && GAME_JS.contains("title=\"hints left\""),
        "scoreboard should show remaining hint counts",
    );
}

#[test]
fn scoreboard_shows_points_per_play() {
    assert!(
        GAME_JS.contains("function pointsPerPlay(seat)")
            && GAME_JS.contains("title=\"Points per play\"")
            && GAME_JS.contains("Pts/Play")
            && GAME_JS.contains("pointsPerPlay(seat)"),
        "scoreboard should include a points-per-play column",
    );
}

#[test]
fn move_log_shows_scott_mode_best_play() {
    assert!(
        GAME_JS.contains("mv.best")
            && GAME_JS.contains("best-play")
            && GAME_JS.contains("Scott Mode"),
        "move log should surface the best available play in Scott Mode",
    );
    assert!(
        GAME_JS.contains("Shelli Mode"),
        "Shelli Mode should have a game badge",
    );
    assert!(
        APP_CSS.contains(".best-play"),
        "best-play annotation should be styled",
    );
}

#[test]
fn pwa_turn_affordance_uses_badging_api() {
    assert!(
        GAME_JS.contains("function setTurnAffordanceSource(source, count)")
            && GAME_JS.contains("function updateTurnAffordances()"),
        "turn affordances should be tracked from multiple game sources",
    );
    assert!(
        GAME_JS.contains("\"setAppBadge\" in navigator")
            && GAME_JS.contains("navigator.setAppBadge(count)")
            && GAME_JS.contains("\"clearAppBadge\" in navigator")
            && GAME_JS.contains("navigator.clearAppBadge()"),
        "PWA icon affordance should use the guarded Badging API",
    );
    assert!(
        GAME_JS.contains("setTurnAffordanceSource(\"current-game\", yourTurn ? 1 : 0)")
            && GAME_JS.contains("setTurnAffordanceSource(\"other-games\", nowTurn.size)"),
        "badge count should include the current game and other games where it is your turn",
    );
}

#[test]
fn web_push_notification_flow_is_wired() {
    assert!(
        GAME_JS.contains("navigator.serviceWorker.register(\"/sw.js\")")
            && GAME_JS.contains("registration.pushManager.subscribe")
            && GAME_JS.contains("applicationServerKey: urlBase64ToUint8Array(publicKey)")
            && GAME_JS.contains("fetch(\"/api/push/subscribe\""),
        "game page should register the service worker and persist push subscriptions",
    );
    assert!(
        GAME_JS.contains("fetch(\"/api/push/vapid-public-key\")")
            && GAME_JS.contains("Enable notifications"),
        "game page should expose an opt-in notification flow",
    );
    assert!(
        SW_JS.contains("self.addEventListener(\"push\"")
            && SW_JS.contains("self.registration.showNotification")
            && SW_JS.contains("self.addEventListener(\"notificationclick\"")
            && SW_JS.contains("self.clients.openWindow(url)"),
        "service worker should show push notifications and open the target game",
    );
    assert!(
        NOTIFICATION_DEBUG_JS.contains("registration.showNotification")
            && NOTIFICATION_DEBUG_JS.contains("\"/api/push/debug\"")
            && NOTIFICATION_DEBUG_JS.contains("\"/api/push/test\"")
            && NOTIFICATION_DEBUG_JS.contains("\"/api/push/subscribe\"")
            && NOTIFICATION_DEBUG_JS.contains("\"/api/push/unsubscribe\""),
        "notification debug page should exercise browser display, subscription storage, and server push paths",
    );
    assert!(
        css_rule_contains(".debug-grid", "display: grid;")
            && css_rule_contains(".debug-output", "white-space: pre-wrap;")
            && APP_CSS.contains(".debug-grid { grid-template-columns: 1fr; }"),
        "notification debug diagnostics should be readable on desktop and mobile",
    );
}

#[test]
fn touch_debug_page_asset_is_wired() {
    assert!(
        TOUCH_DEBUG_JS.contains("const STORAGE_KEY = \"screwball.touchDebug.offsets.v1\"")
            && TOUCH_DEBUG_JS.contains("function proposedOffsets()")
            && TOUCH_DEBUG_JS.contains("function updateDragVisuals")
            && TOUCH_DEBUG_JS.contains("showBoardDropGhost"),
        "touch debug script should persist offsets and render live drag/drop markers",
    );
    assert!(
        APP_CSS.contains(".touch-debug {")
            && APP_CSS.contains(".touch-debug-controls")
            && APP_CSS.contains(".touch-debug-board")
            && APP_CSS.contains(".touch-debug-rack"),
        "touch debug page should have dedicated responsive layout and controls",
    );
}

fn css_rule_contains(selector: &str, declaration: &str) -> bool {
    let selector_start = format!("{selector} {{");
    let Some(start) = APP_CSS.find(&selector_start) else {
        return false;
    };
    let Some(open_brace) = APP_CSS[start..].find('{') else {
        return false;
    };
    let body_start = start + open_brace + 1;
    let Some(close_brace) = APP_CSS[body_start..].find('}') else {
        return false;
    };
    APP_CSS[body_start..body_start + close_brace].contains(declaration)
}
