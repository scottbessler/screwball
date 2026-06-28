const GAME_JS: &str = include_str!("../public/game.js");
const APP_CSS: &str = include_str!("../public/app.css");

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
            && GAME_JS.contains("h(\"div\", { class: \"board-wrap\" }, [")
            && GAME_JS.contains("h(\"div\", { class: \"rack-area\" }, ["),
        "layout wrappers should be explicit h() divs instead of top-level htm div templates",
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
        css_rule_contains(".rack-tile", "color: var(--ink);"),
        "rack tile buttons should override mobile default button/link text color",
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
