import {
  html,
  render,
  useState,
  useEffect,
  useRef,
} from "/public/vendor/htm-preact.js";

const SIZE = 15;
const CENTER = 7;

const POINTS = {
  A: 1, B: 3, C: 3, D: 2, E: 1, F: 4, G: 2, H: 4, I: 1, J: 8,
  K: 5, L: 1, M: 3, N: 1, O: 1, P: 3, Q: 10, R: 1, S: 1, T: 1,
  U: 1, V: 4, W: 4, X: 8, Y: 4, Z: 10,
};

const PREMIUM_LABEL = { dl: "DL", tl: "TL", dw: "DW", tw: "TW", none: "" };
const LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".split("");

function pointsFor(letter, isBlank) {
  return isBlank ? 0 : POINTS[letter] || 0;
}

function idx(row, col) {
  return row * SIZE + col;
}

function isActive(game) {
  return game.status === "Active";
}

function isYourTurn(game) {
  return (
    isActive(game) &&
    game.your_seat !== null &&
    game.turn === game.your_seat
  );
}

function rackSignature(game) {
  return (game.your_rack || [])
    .map((t) => (t.is_blank ? "?" : t.letter))
    .join("");
}

// A board tile rendered for a committed letter or a pending placement.
function Tile({ letter, isBlank, points, pending, onClick }) {
  const cls = ["tile-face", isBlank ? "tile-blank" : "", pending ? "pending" : ""]
    .filter(Boolean)
    .join(" ");
  return html`<span class=${cls} onClick=${onClick}>
    <span class="tile-letter">${letter}</span>
    <span class="tile-points">${points}</span>
  </span>`;
}

function Cell({ square, pending, row, col, cursor, onClick }) {
  if (square.letter) {
    return html`<div class="cell tile">
      <${Tile}
        letter=${square.letter}
        isBlank=${square.is_blank}
        points=${pointsFor(square.letter, square.is_blank)}
      />
    </div>`;
  }
  if (pending) {
    return html`<div class="cell tile" onClick=${onClick}>
      <${Tile}
        letter=${pending.letter}
        isBlank=${pending.isBlank}
        points=${pointsFor(pending.letter, pending.isBlank)}
        pending
      />
    </div>`;
  }
  const premium = square.premium;
  const isCursor = cursor && cursor.row === row && cursor.col === col;
  const cls = [
    "cell",
    premium === "none" ? "" : `premium-${premium}`,
    isCursor ? "cursor" : "",
  ]
    .filter(Boolean)
    .join(" ");
  const star = row === CENTER && col === CENTER ? "★" : PREMIUM_LABEL[premium];
  return html`<div class=${cls} onClick=${onClick}>
    ${isCursor
      ? html`<span class="cursor-arrow">${cursor.dir === "down" ? "↓" : "→"}</span>`
      : html`<span class="premium-label">${star}</span>`}
  </div>`;
}

function Board({ game, pending, cursor, onCellClick, onPendingClick }) {
  const byPos = new Map(pending.map((p) => [idx(p.row, p.col), p]));
  const cells = [];
  for (let row = 0; row < SIZE; row++) {
    for (let col = 0; col < SIZE; col++) {
      const i = idx(row, col);
      const place = byPos.get(i);
      cells.push(
        html`<${Cell}
          key=${i}
          square=${game.board[i]}
          pending=${place}
          row=${row}
          col=${col}
          cursor=${cursor}
          onClick=${() =>
            place ? onPendingClick(place) : onCellClick(row, col)}
        />`,
      );
    }
  }
  return html`<div class="board" role="grid">${cells}</div>`;
}

function Rack({ tiles, selected, mode, exchange, onSelect, onReorder }) {
  const dragId = useRef(null);
  return html`<div class="rack">
    ${tiles.map((tile) => {
      const picked =
        mode === "exchange" ? exchange.has(tile.id) : selected === tile.id;
      const cls = [
        "rack-tile",
        tile.is_blank ? "tile-blank" : "",
        picked ? "selected" : "",
      ]
        .filter(Boolean)
        .join(" ");
      return html`<button
        type="button"
        key=${tile.id}
        class=${cls}
        draggable=${true}
        onDragStart=${() => {
          dragId.current = tile.id;
        }}
        onDragOver=${(e) => e.preventDefault()}
        onDrop=${(e) => {
          e.preventDefault();
          if (dragId.current !== null && dragId.current !== tile.id) {
            onReorder(dragId.current, tile.id);
          }
          dragId.current = null;
        }}
        onClick=${() => onSelect(tile)}
      >
        <span class="tile-letter">${tile.is_blank ? " " : tile.letter}</span>
        <span class="tile-points">${pointsFor(tile.letter, tile.is_blank)}</span>
      </button>`;
    })}
  </div>`;
}

function Scoreboard({ game }) {
  return html`<table class="scoreboard">
    <thead>
      <tr><th>Player</th><th>Type</th><th>Score</th></tr>
    </thead>
    <tbody>
      ${game.seats.map(
        (seat) => html`<tr class=${seat.on_turn ? "seat on-turn" : "seat"}>
          <td>
            ${seat.name}
            ${seat.is_you ? html`<span class="badge">you</span>` : null}
          </td>
          <td class="muted">
            ${seat.kind === "bot" ? `${seat.difficulty} bot` : "human"}
          </td>
          <td class="score">${seat.score}</td>
        </tr>`,
      )}
    </tbody>
  </table>`;
}

function MoveLog({ game }) {
  if (!game.moves.length) {
    return html`<p class="muted">No moves yet.</p>`;
  }
  const recent = game.moves.slice(-10).reverse();
  return html`<ul class="move-log">
    ${recent.map((mv, n) => {
      const name = game.seats[mv.seat] ? game.seats[mv.seat].name : "?";
      let detail;
      if (mv.kind === "play") {
        detail = `${mv.words.join(", ")} (+${mv.points})`;
      } else if (mv.kind === "exchange") {
        detail = "exchanged tiles";
      } else if (mv.kind === "adjustment") {
        detail =
          mv.delta >= 0
            ? `out bonus (+${mv.delta})`
            : `leftover ${mv.words.join("")} (${mv.delta})`;
      } else {
        detail = "passed";
      }
      return html`<li key=${n}><strong>${name}</strong>: ${detail}</li>`;
    })}
  </ul>`;
}

function Results({ game }) {
  const ranked = game.seats
    .map((seat) => seat)
    .slice()
    .sort((a, b) => b.score - a.score);
  const winners = new Set(game.winners);
  return html`<div class="results card">
    <h2>Game over</h2>
    <ol class="results-list">
      ${ranked.map(
        (seat) => html`<li
          key=${seat.index}
          class=${winners.has(seat.index) ? "winner" : ""}
        >
          <span class="results-name">
            ${seat.name}
            ${winners.has(seat.index) ? html`<span class="badge">winner</span>` : null}
          </span>
          <span class="results-score">${seat.score}</span>
        </li>`,
      )}
    </ol>
    <a class="button" href="/">New game</a>
  </div>`;
}

function BlankPicker({ onPick, onCancel }) {
  useEffect(() => {
    function onKey(e) {
      if (e.key === "Escape") {
        e.preventDefault();
        onCancel();
      } else if (/^[a-zA-Z]$/.test(e.key)) {
        e.preventDefault();
        onPick(e.key.toUpperCase());
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onPick, onCancel]);

  return html`<div class="modal-backdrop" onClick=${onCancel}>
    <div class="modal" onClick=${(e) => e.stopPropagation()}>
      <h2>Choose a letter</h2>
      <p class="muted">Pick the letter this blank tile represents.</p>
      <div class="letter-grid">
        ${LETTERS.map(
          (l) => html`<button
            type="button"
            key=${l}
            class="letter-btn"
            onClick=${() => onPick(l)}
          >
            ${l}
          </button>`,
        )}
      </div>
      <button type="button" class="button ghost" onClick=${onCancel}>
        Cancel
      </button>
    </div>
  </div>`;
}

function JoinForm({ gameId }) {
  return html`<form class="join-form" method="post" action=${`/games/${gameId}/join`}>
    <p class="muted">An open seat is waiting. Join to play.</p>
    <label>Your name
      <input type="text" name="name" maxlength="24" placeholder="You" />
    </label>
    <button type="submit" class="button">Join game</button>
  </form>`;
}

function statusText(game) {
  if (game.status === "Lobby") return "Waiting to start";
  if (game.status === "Finished") {
    const names = game.winners
      .map((i) => (game.seats[i] ? game.seats[i].name : null))
      .filter(Boolean);
    return names.length ? `Game over — winner: ${names.join(", ")}` : "Game over";
  }
  const seat = game.seats[game.turn];
  if (seat && seat.is_you) return "Your turn";
  return seat ? `${seat.name}'s turn` : "In progress";
}

function App({ gameId, initial }) {
  const [game, setGame] = useState(initial);
  const [pending, setPending] = useState([]);
  const [selected, setSelected] = useState(null);
  const [mode, setMode] = useState("place");
  const [exchange, setExchange] = useState(() => new Set());
  const [error, setError] = useState(null);
  const [busy, setBusy] = useState(false);
  const [cursor, setCursor] = useState(null);
  const [blankPrompt, setBlankPrompt] = useState(null);
  const [rackOrder, setRackOrder] = useState(() =>
    (initial.your_rack || []).map((_, i) => i),
  );

  const yourTurn = isYourTurn(game);
  const seated = game.your_seat !== null && game.your_seat !== undefined;
  const hasOpenSeat = game.seats.some((s) => s.open);

  // Poll for opponent/bot moves while we are waiting.
  useEffect(() => {
    if (yourTurn || game.status === "Finished") return undefined;
    const timer = setInterval(async () => {
      try {
        const res = await fetch(`/games/${gameId}/state`);
        if (!res.ok) return;
        const next = await res.json();
        setGame(next);
      } catch (_) {
        /* transient network error; keep polling */
      }
    }, 2500);
    return () => clearInterval(timer);
  }, [gameId, yourTurn, game.status]);

  // Reset the rack display order whenever the underlying rack changes.
  const sig = rackSignature(game);
  useEffect(() => {
    setRackOrder((game.your_rack || []).map((_, i) => i));
  }, [sig]);

  const usedRackIds = new Set(pending.map((p) => p.rackId));
  const rackTiles = game.your_rack || [];
  const order =
    rackOrder.length === rackTiles.length
      ? rackOrder
      : rackTiles.map((_, i) => i);
  const rack = order
    .filter((id) => !usedRackIds.has(id))
    .map((id) => ({
      id,
      letter: rackTiles[id].letter,
      is_blank: rackTiles[id].is_blank,
    }));

  function reset() {
    setPending([]);
    setSelected(null);
    setMode("place");
    setExchange(new Set());
    setCursor(null);
  }

  function occupiedAt(row, col, pend) {
    if (game.board[idx(row, col)].letter) return true;
    return pend.some((p) => p.row === row && p.col === col);
  }

  // First empty square at or after (row,col) travelling in `dir`.
  function firstEmptyFrom(row, col, dir, pend) {
    let r = row;
    let c = col;
    while (r >= 0 && r < SIZE && c >= 0 && c < SIZE) {
      if (!occupiedAt(r, c, pend)) return { row: r, col: c };
      if (dir === "down") r += 1;
      else c += 1;
    }
    return null;
  }

  function selectTile(tile) {
    setError(null);
    if (mode === "exchange") {
      const next = new Set(exchange);
      if (next.has(tile.id)) next.delete(tile.id);
      else next.add(tile.id);
      setExchange(next);
      return;
    }
    setSelected((prev) => (prev === tile.id ? null : tile.id));
  }

  function reorderRack(fromId, toId) {
    const next = order.slice();
    const from = next.indexOf(fromId);
    const to = next.indexOf(toId);
    if (from === -1 || to === -1) return;
    next.splice(from, 1);
    next.splice(to, 0, fromId);
    setRackOrder(next);
  }

  function shuffleRack() {
    const next = order.slice();
    for (let i = next.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [next[i], next[j]] = [next[j], next[i]];
    }
    setRackOrder(next);
  }

  // Click flow: a rack tile is selected, drop it on a board square.
  function placeSelected(row, col) {
    const tile = rackTiles[selected];
    if (!tile) return;
    if (tile.is_blank) {
      setBlankPrompt({ row, col, rackId: selected });
      return;
    }
    setPending([
      ...pending,
      { row, col, letter: tile.letter, isBlank: false, rackId: selected },
    ]);
    setSelected(null);
  }

  // Keyboard flow: manage the typing cursor on board clicks.
  function moveCursor(row, col) {
    if (cursor && cursor.row === row && cursor.col === col) {
      setCursor({ row, col, dir: cursor.dir === "right" ? "down" : "right" });
      return;
    }
    if (cursor && row === cursor.row && col === cursor.col + 1) {
      setCursor({ ...cursor, dir: "right" });
      return;
    }
    if (cursor && col === cursor.col && row === cursor.row + 1) {
      setCursor({ ...cursor, dir: "down" });
      return;
    }
    setCursor({ row, col, dir: "right" });
  }

  function onCellClick(row, col) {
    if (!yourTurn || mode === "exchange") return;
    setError(null);
    if (game.board[idx(row, col)].letter) return;
    if (selected !== null) {
      placeSelected(row, col);
      return;
    }
    moveCursor(row, col);
  }

  function typeLetter(letter) {
    if (!cursor) {
      setError("Click a square to start a word, then type.");
      return;
    }
    const target = firstEmptyFrom(cursor.row, cursor.col, cursor.dir, pending);
    if (!target) {
      setError("No room to place a tile that way.");
      return;
    }
    const exact = rack.find((t) => !t.is_blank && t.letter === letter);
    const blank = rack.find((t) => t.is_blank);
    const chosen = exact || blank;
    if (!chosen) {
      setError(`No "${letter}" tile (or blank) on your rack.`);
      return;
    }
    const next = [
      ...pending,
      {
        row: target.row,
        col: target.col,
        letter,
        isBlank: !exact,
        rackId: chosen.id,
      },
    ];
    setPending(next);
    const after =
      cursor.dir === "down"
        ? { r: target.row + 1, c: target.col }
        : { r: target.row, c: target.col + 1 };
    const advanced = firstEmptyFrom(after.r, after.c, cursor.dir, next);
    setCursor(advanced ? { ...advanced, dir: cursor.dir } : cursor);
  }

  function backspace() {
    if (!pending.length) return;
    const last = pending[pending.length - 1];
    setPending(pending.slice(0, -1));
    setCursor({ row: last.row, col: last.col, dir: cursor ? cursor.dir : "right" });
  }

  // Type-to-place keyboard handling. Use a ref so the listener always sees
  // fresh state without rebinding on every keystroke.
  const keyHandler = useRef(null);
  keyHandler.current = (e) => {
    if (!yourTurn || mode !== "place" || blankPrompt) return;
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const tag = e.target && e.target.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA") return;
    if (/^[a-zA-Z]$/.test(e.key)) {
      e.preventDefault();
      typeLetter(e.key.toUpperCase());
    } else if (e.key === "Backspace") {
      e.preventDefault();
      backspace();
    } else if (e.key === "Enter") {
      e.preventDefault();
      submitPlay();
    } else if (e.key === "Escape") {
      e.preventDefault();
      setPending([]);
      setSelected(null);
      setCursor(null);
      setError(null);
    }
  };
  useEffect(() => {
    function onKey(e) {
      if (keyHandler.current) keyHandler.current(e);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  function recallTile(place) {
    setPending(pending.filter((p) => p.rackId !== place.rackId));
  }

  async function postMove(body) {
    setBusy(true);
    setError(null);
    try {
      const res = await fetch(`/games/${gameId}/move`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      const data = await res.json();
      if (!res.ok) {
        setError(data.error || "Move rejected.");
        return;
      }
      setGame(data);
      reset();
    } catch (_) {
      setError("Network error — try again.");
    } finally {
      setBusy(false);
    }
  }

  function submitPlay() {
    if (!pending.length) {
      setError("Place at least one tile first.");
      return;
    }
    postMove({
      kind: "play",
      placements: pending.map((p) => ({
        row: p.row,
        col: p.col,
        letter: p.letter,
        is_blank: p.isBlank,
      })),
    });
  }

  function submitExchange() {
    if (!exchange.size) {
      setError("Select rack tiles to exchange.");
      return;
    }
    const tiles = [...exchange].map((id) => {
      const tile = rackTiles[id];
      return tile.is_blank ? "?" : tile.letter;
    });
    postMove({ kind: "exchange", tiles });
  }

  const controlsDisabled = !yourTurn || busy;
  const finished = game.status === "Finished";

  return html`<div class="game">
    <h1 class="status">${statusText(game)}</h1>
    ${finished ? html`<${Results} game=${game} />` : null}
    <div class="game-layout">
      <div class="board-wrap">
        <${Board}
          game=${game}
          pending=${pending}
          cursor=${yourTurn && mode === "place" ? cursor : null}
          onCellClick=${onCellClick}
          onPendingClick=${recallTile}
        />
        ${!seated && hasOpenSeat ? html`<${JoinForm} gameId=${gameId} />` : null}
        ${seated && !finished
          ? html`<div class="rack-area">
              <${Rack}
                tiles=${rack}
                selected=${selected}
                mode=${mode}
                exchange=${exchange}
                onSelect=${selectTile}
                onReorder=${reorderRack}
              />
              ${error ? html`<p class="move-error">${error}</p>` : null}
              <div class="controls">
                ${mode === "place"
                  ? html`<button
                        type="button"
                        class="button"
                        disabled=${controlsDisabled}
                        onClick=${submitPlay}
                      >
                        Play word
                      </button>
                      <button
                        type="button"
                        class="button ghost"
                        disabled=${busy || !pending.length}
                        onClick=${() => {
                          setPending([]);
                          setCursor(null);
                        }}
                      >
                        Recall
                      </button>
                      <button
                        type="button"
                        class="button ghost"
                        disabled=${busy}
                        onClick=${shuffleRack}
                      >
                        Shuffle
                      </button>
                      <button
                        type="button"
                        class="button ghost"
                        disabled=${controlsDisabled}
                        onClick=${() => {
                          reset();
                          setMode("exchange");
                        }}
                      >
                        Exchange…
                      </button>
                      <button
                        type="button"
                        class="button ghost"
                        disabled=${controlsDisabled}
                        onClick=${() => postMove({ kind: "pass" })}
                      >
                        Pass
                      </button>`
                  : html`<button
                        type="button"
                        class="button"
                        disabled=${controlsDisabled}
                        onClick=${submitExchange}
                      >
                        Confirm exchange
                      </button>
                      <button
                        type="button"
                        class="button ghost"
                        disabled=${busy}
                        onClick=${reset}
                      >
                        Cancel
                      </button>`}
              </div>
              ${mode === "place" && yourTurn
                ? html`<p class="muted hint">
                    Click a square to start a word, then type letters (click an
                    adjacent square to set direction, or click again to toggle
                    →/↓). Or tap a rack tile, then a square. Drag rack tiles to
                    reorder; Shuffle to randomize. Enter plays, Esc clears.
                  </p>`
                : null}
            </div>`
          : null}
      </div>
      <aside class="sidebar">
        <${Scoreboard} game=${game} />
        <p class="muted">Tiles in bag: ${game.bag_count}</p>
        <${MoveLog} game=${game} />
      </aside>
    </div>
    ${blankPrompt
      ? html`<${BlankPicker}
          onPick=${(letter) => {
            setPending([
              ...pending,
              {
                row: blankPrompt.row,
                col: blankPrompt.col,
                letter,
                isBlank: true,
                rackId: blankPrompt.rackId,
              },
            ]);
            setBlankPrompt(null);
            setSelected(null);
          }}
          onCancel=${() => setBlankPrompt(null)}
        />`
      : null}
  </div>`;
}

function boot() {
  const mount = document.getElementById("game-island");
  const stateEl = document.getElementById("game-state");
  if (!mount || !stateEl) return;
  const initial = JSON.parse(stateEl.textContent);
  const fallback = document.getElementById("ssr-fallback");
  if (fallback) fallback.hidden = true;
  render(
    html`<${App} gameId=${mount.dataset.gameId} initial=${initial} />`,
    mount,
  );
}

boot();
