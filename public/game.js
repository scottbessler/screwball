import {
  html,
  render,
  useState,
  useEffect,
  useRef,
  useMemo,
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

// Valid 2-letter words, embedded by the server for John Mode's helper.
const TWO_LETTER_WORDS = (() => {
  const el = document.getElementById("two-letter-words");
  try {
    return el ? JSON.parse(el.textContent) : [];
  } catch {
    return [];
  }
})();

function pointsFor(letter, isBlank) {
  return isBlank ? 0 : POINTS[letter] || 0;
}

function idx(row, col) {
  return row * SIZE + col;
}

// Live score of the pending placements. Mirrors score logic in src/game.rs
// (main run + cross words + 50 bingo). No dict check — server validates words.
function previewScore(game, pending) {
  if (!pending.length) return null;
  const placed = new Map(pending.map((p) => [idx(p.row, p.col), p]));
  const at = (r, c) => {
    if (r < 0 || r >= SIZE || c < 0 || c >= SIZE) return null;
    const sq = game.board[idx(r, c)];
    if (sq.letter) return { letter: sq.letter, isBlank: sq.is_blank, premium: sq.premium, placed: false };
    const p = placed.get(idx(r, c));
    return p ? { letter: p.letter, isBlank: p.isBlank, premium: sq.premium, placed: true } : null;
  };
  const run = ([sr, sc], [dr, dc]) => {
    let r = sr, c = sc;
    while (at(r - dr, c - dc)) { r -= dr; c -= dc; }
    const cells = [];
    while (at(r, c)) { cells.push([r, c]); r += dr; c += dc; }
    return cells;
  };
  const multiCol = pending.some((p) => p.col !== pending[0].col);
  const multiRow = pending.some((p) => p.row !== pending[0].row);
  const [main, cross] = multiCol
    ? [[0, 1], [1, 0]]
    : multiRow
      ? [[1, 0], [0, 1]]
      : [[0, 1], [1, 0]];
  const words = [];
  const head = [pending[0].row, pending[0].col];
  const m = run(head, main);
  if (m.length >= 2) words.push(m);
  for (const p of pending) {
    const x = run([p.row, p.col], cross);
    if (x.length >= 2) words.push(x);
  }
  let total = 0;
  for (const cells of words) {
    let ws = 0, wm = 1;
    for (const [r, c] of cells) {
      const t = at(r, c);
      let v = pointsFor(t.letter, t.isBlank);
      if (t.placed) {
        if (t.premium === "dl") v *= 2;
        else if (t.premium === "tl") v *= 3;
        else if (t.premium === "dw") wm *= 2;
        else if (t.premium === "tw") wm *= 3;
      }
      ws += v;
    }
    total += ws * wm;
  }
  if (pending.length === 7) total += 50;
  return total;
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
function Tile({ letter, isBlank, points, pending, lastPlay, onClick }) {
  const cls = ["tile-face", isBlank ? "tile-blank" : "", pending ? "pending" : "", lastPlay || ""]
    .filter(Boolean)
    .join(" ");
  return html`<span class=${cls} onClick=${onClick}>
    <span class="tile-letter">${letter}</span>
    <span class="tile-points">${points}</span>
  </span>`;
}

function Cell({ square, pending, row, col, cursor, lastPlay, onClick, onDragOver, onDrop }) {
  if (square.letter) {
    const lp = lastPlay ? "last-play" : "";
    return html`<div class="cell tile" data-row=${row} data-col=${col}>
      <${Tile}
        letter=${square.letter}
        isBlank=${square.is_blank}
        points=${pointsFor(square.letter, square.is_blank)}
        lastPlay=${lp}
      />
    </div>`;
  }
  if (pending) {
    return html`<div class="cell tile" data-row=${row} data-col=${col} onClick=${onClick}>
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
  return html`<div class=${cls} data-row=${row} data-col=${col} onClick=${onClick}
    onDragOver=${onDragOver} onDrop=${onDrop}>
    ${isCursor
      ? html`<span class="cursor-arrow">${cursor.dir === "down" ? "↓" : "→"}</span>`
      : html`<span class="premium-label">${star}</span>`}
  </div>`;
}

function Board({ game, pending, cursor, lastPlaySet, onCellClick, onPendingClick, onDropTile }) {
  const byPos = new Map(pending.map((p) => [idx(p.row, p.col), p]));
  const boardRef = useRef(null);
  const pinchState = useRef(null);
  const scaleRef = useRef(1);
  const panRef = useRef({ x: 0, y: 0 });
  const panStartRef = useRef(null);

  function applyTransform() {
    if (!boardRef.current) return;
    const s = scaleRef.current;
    const { x, y } = panRef.current;
    boardRef.current.style.transform = s === 1 && x === 0 && y === 0
      ? "" : `translate(${x}px, ${y}px) scale(${s})`;
  }

  function handleBoardTouchStart(e) {
    if (e.touches.length === 2) {
      e.preventDefault();
      const dx = e.touches[1].clientX - e.touches[0].clientX;
      const dy = e.touches[1].clientY - e.touches[0].clientY;
      const midX = (e.touches[0].clientX + e.touches[1].clientX) / 2;
      const midY = (e.touches[0].clientY + e.touches[1].clientY) / 2;
      const rect = boardRef.current.getBoundingClientRect();
      const ox = ((midX - rect.left) / rect.width) * 100;
      const oy = ((midY - rect.top) / rect.height) * 100;
      boardRef.current.style.transformOrigin = `${ox}% ${oy}%`;
      pinchState.current = {
        dist: Math.hypot(dx, dy),
        baseScale: scaleRef.current,
      };
      panStartRef.current = null;
    } else if (e.touches.length === 1 && scaleRef.current > 1) {
      panStartRef.current = {
        x: e.touches[0].clientX,
        y: e.touches[0].clientY,
        basePanX: panRef.current.x,
        basePanY: panRef.current.y,
      };
    }
  }

  function handleBoardTouchMove(e) {
    if (e.touches.length === 2 && pinchState.current) {
      e.preventDefault();
      const dx = e.touches[1].clientX - e.touches[0].clientX;
      const dy = e.touches[1].clientY - e.touches[0].clientY;
      const dist = Math.hypot(dx, dy);
      const ratio = dist / pinchState.current.dist;
      const newScale = Math.min(Math.max(pinchState.current.baseScale * ratio, 1), 3);
      scaleRef.current = newScale;
      applyTransform();
    } else if (e.touches.length === 1 && panStartRef.current && scaleRef.current > 1) {
      const dx = e.touches[0].clientX - panStartRef.current.x;
      const dy = e.touches[0].clientY - panStartRef.current.y;
      if (Math.abs(dx) + Math.abs(dy) > 5) {
        e.preventDefault();
        panRef.current = {
          x: panStartRef.current.basePanX + dx,
          y: panStartRef.current.basePanY + dy,
        };
        applyTransform();
      }
    }
  }

  function handleBoardTouchEnd(e) {
    if (e.touches.length < 2) {
      pinchState.current = null;
      if (scaleRef.current < 1.1) {
        scaleRef.current = 1;
        panRef.current = { x: 0, y: 0 };
        if (boardRef.current) {
          boardRef.current.style.transform = "";
          boardRef.current.style.transformOrigin = "";
        }
      }
    }
    if (e.touches.length === 0) {
      panStartRef.current = null;
    }
  }

  // Double-tap to reset zoom
  const lastTapRef = useRef(0);
  function handleDoubleTap(e) {
    const now = Date.now();
    if (now - lastTapRef.current < 300 && e.touches.length === 1) {
      scaleRef.current = 1;
      panRef.current = { x: 0, y: 0 };
      if (boardRef.current) {
        boardRef.current.style.transform = "";
        boardRef.current.style.transformOrigin = "";
      }
    }
    lastTapRef.current = now;
  }

  const cells = [];
  for (let row = 0; row < SIZE; row++) {
    for (let col = 0; col < SIZE; col++) {
      const i = idx(row, col);
      const place = byPos.get(i);
      const isEmpty = !game.board[i].letter && !place;
      cells.push(
        html`<${Cell}
          key=${i}
          square=${game.board[i]}
          pending=${place}
          row=${row}
          col=${col}
          cursor=${cursor}
          lastPlay=${lastPlaySet && lastPlaySet.has(i)}
          onClick=${() =>
            place ? onPendingClick(place) : onCellClick(row, col)}
          onDragOver=${isEmpty ? (e) => e.preventDefault() : null}
          onDrop=${isEmpty ? (e) => {
            e.preventDefault();
            const tileId = Number(e.dataTransfer.getData("text/plain"));
            if (onDropTile) onDropTile(tileId, row, col);
          } : null}
        />`,
      );
    }
  }
  return html`<div class="board" role="grid" ref=${boardRef}
    onTouchStart=${(e) => { handleDoubleTap(e); handleBoardTouchStart(e); }}
    onTouchMove=${handleBoardTouchMove}
    onTouchEnd=${handleBoardTouchEnd}
  >${cells}</div>`;
}

// Find an empty board cell at/near a screen point. Probes a small radius so a
// touch that lands slightly off a square still resolves to it (forgiving drop).
function cellAtPoint(x, y) {
  const TOL = 22;
  const offsets = [
    [0, 0],
    [0, -TOL], [0, TOL], [-TOL, 0], [TOL, 0],
    [-TOL, -TOL], [TOL, -TOL], [-TOL, TOL], [TOL, TOL],
  ];
  for (const [ox, oy] of offsets) {
    const el = document.elementFromPoint(x + ox, y + oy);
    const cell = el && el.closest(".cell");
    if (
      cell &&
      cell.dataset.row != null &&
      cell.dataset.col != null &&
      !cell.classList.contains("tile")
    ) {
      return cell;
    }
  }
  return null;
}

function Rack({ tiles, selected, mode, exchange, onSelect, onReorder, onPlaceOnBoard, onHover }) {
  const dragId = useRef(null);
  const touchState = useRef(null);
  const rackRef = useRef(null);

  function handleTouchStart(e, tile) {
    e.preventDefault();
    const touch = e.touches[0];
    touchState.current = {
      id: tile.id,
      originalId: tile.id,
      startX: touch.clientX,
      startY: touch.clientY,
      dragging: false,
      ghost: null,
      sourceEl: e.currentTarget,
    };
  }

  function handleTouchMove(e) {
    if (!touchState.current) return;
    const touch = e.touches[0];
    const dx = touch.clientX - touchState.current.startX;
    const dy = touch.clientY - touchState.current.startY;
    if (!touchState.current.dragging && Math.abs(dx) + Math.abs(dy) > 8) {
      touchState.current.dragging = true;
      touchState.current.sourceEl.classList.add("dragging");
      const tile = tiles.find((t) => t.id === touchState.current.id);
      const ghost = document.createElement("div");
      ghost.className = "rack-tile-ghost";
      ghost.textContent = tile ? (tile.is_blank ? " " : tile.letter) : "";
      document.body.appendChild(ghost);
      touchState.current.ghost = ghost;
    }
    if (touchState.current.dragging) {
      e.preventDefault();
      const ghost = touchState.current.ghost;
      if (ghost) {
        ghost.style.left = touch.clientX + "px";
        ghost.style.top = touch.clientY + "px";
      }
      // Clear previous highlight
      if (touchState.current.highlightedCell) {
        touchState.current.highlightedCell.classList.remove("drag-over");
        touchState.current.highlightedCell = null;
      }
      const el = document.elementFromPoint(touch.clientX, touch.clientY);
      if (el) {
        const btn = el.closest(".rack-tile");
        if (btn && btn.dataset.tileId != null) {
          const targetId = Number(btn.dataset.tileId);
          if (targetId !== touchState.current.id) {
            onReorder(touchState.current.id, targetId);
            touchState.current.id = targetId;
            btn.classList.remove("reorder-pop");
            void btn.offsetWidth;
            btn.classList.add("reorder-pop");
          }
        }
      }
      // Highlight the nearest board cell (forgiving of a slightly-off touch).
      const cell = cellAtPoint(touch.clientX, touch.clientY);
      if (cell) {
        cell.classList.add("drag-over");
        touchState.current.highlightedCell = cell;
      }
    }
  }

  function handleTouchEnd(e) {
    if (touchState.current) {
      if (touchState.current.ghost) {
        touchState.current.ghost.remove();
      }
      if (touchState.current.sourceEl) {
        touchState.current.sourceEl.classList.remove("dragging");
      }
      if (touchState.current.highlightedCell) {
        touchState.current.highlightedCell.classList.remove("drag-over");
      }
      if (touchState.current.dragging) {
        // Drop onto the nearest board cell (forgiving of a slightly-off touch).
        const lastTouch = e.changedTouches[0];
        const cell = cellAtPoint(lastTouch.clientX, lastTouch.clientY);
        if (cell) {
          const row = Number(cell.dataset.row);
          const col = Number(cell.dataset.col);
          const tile = tiles.find((t) => t.id === touchState.current.originalId);
          if (tile && onPlaceOnBoard) {
            onPlaceOnBoard(tile, row, col);
          }
        }
      } else {
        e.preventDefault();
        const tile = tiles.find((t) => t.id === touchState.current.id);
        if (tile) onSelect(tile);
      }
    }
    touchState.current = null;
  }

  return html`<div class="rack" ref=${rackRef}
    onTouchMove=${handleTouchMove}
    onTouchEnd=${handleTouchEnd}
  >
    ${tiles.map((tile) => {
      // Placed tile: hold its slot with an inert placeholder so nothing reflows.
      if (tile.used) {
        return html`<div class="rack-tile rack-slot" key=${tile.id}></div>`;
      }
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
        data-tile-id=${tile.id}
        draggable=${true}
        onDragStart=${(e) => {
          dragId.current = tile.id;
          e.dataTransfer.setData("text/plain", String(tile.id));
          e.dataTransfer.effectAllowed = "move";
        }}
        onDragOver=${(e) => e.preventDefault()}
        onDrop=${(e) => {
          e.preventDefault();
          if (dragId.current !== null && dragId.current !== tile.id) {
            onReorder(dragId.current, tile.id);
          }
          dragId.current = null;
        }}
        onTouchStart=${(e) => handleTouchStart(e, tile)}
        onClick=${() => onSelect(tile)}
        onMouseEnter=${() => onHover && onHover(tile.is_blank ? null : tile.letter)}
        onMouseLeave=${() => onHover && onHover(null)}
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
            ${seat.hints_remaining != null
              ? html`<span class="hint-count" title="hints left">💡${seat.hints_remaining}</span>`
              : null}
          </td>
          <td class="muted">
            ${seat.open
              ? "open"
              : seat.kind === "bot"
                ? `${seat.difficulty} bot`
                : "human"}
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
  const recent = game.moves.slice(-10).toReversed();
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
  const ranked = game.seats.toSorted((a, b) => b.score - a.score);
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
    <button type="submit" class="button">Join game</button>
  </form>`;
}

// The "your other games" panel. Polls /api/my-games so the your-turn flags stay
// fresh while you play, and excludes the game currently being viewed.
function OtherGames({ gameId }) {
  const [games, setGames] = useState(null);
  const prevTurnIds = useRef(null);
  useEffect(() => {
    let active = true;
    async function load() {
      try {
        const res = await fetch("/api/my-games");
        if (!res.ok) return;
        const data = await res.json();
        if (active) {
          const others = data.filter((g) => g.id !== gameId);
          const nowTurn = new Set(others.filter((g) => g.your_turn).map((g) => g.id));
          if (prevTurnIds.current !== null) {
            for (const g of others) {
              if (g.your_turn && !prevTurnIds.current.has(g.id)) {
                notifyTurn(
                  `${g.players.join(" vs ")}`,
                  `/games/${g.id}`,
                );
              }
            }
          }
          prevTurnIds.current = nowTurn;
          setGames(data);
        }
      } catch {
        /* transient network error; keep the last list */
      }
    }
    load();
    const timer = setInterval(load, 5000);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, []);

  if (!games) return null;
  const others = games.filter((g) => g.id !== gameId);
  if (!others.length) return null;
  return html`<div class="other-games">
    <h2>Your other games</h2>
    <ul class="other-games-list">
      ${others.map(
        (g) => html`<li key=${g.id} class=${g.your_turn ? "your-turn" : ""}>
          <a href=${`/games/${g.id}`}>${g.players.join(" vs ")}</a>
          ${g.your_turn
            ? html`<span class="badge badge-turn">your turn</span>`
            : html`<span class="muted">${g.status}</span>`}
        </li>`,
      )}
    </ul>
  </div>`;
}

// -- Browser notifications --------------------------------------------------

function requestNotificationPermission() {
  if ("Notification" in window && Notification.permission === "default") {
    Notification.requestPermission();
  }
}

function notifyTurn(body, url) {
  if (!("Notification" in window) || Notification.permission !== "granted") return;
  if (document.visibilityState === "visible") return;
  const n = new Notification("Your turn!", {
    body: body || "It's your turn in Screwball",
    icon: "/public/apple-touch-icon.png",
    tag: url || "screwball-turn",
  });
  n.addEventListener("click", () => {
    if (url) window.location.href = url;
    window.focus();
    n.close();
  });
}

const BASE_FAVICON = "/public/favicon.svg";

function setFavicon(yourTurn) {
  const old = document.querySelector('link[rel="icon"]');
  if (old) old.remove();
  const link = document.createElement("link");
  link.rel = "icon";
  link.type = "image/svg+xml";
  if (!yourTurn) {
    link.href = BASE_FAVICON;
  } else {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
  <rect width="64" height="64" rx="12" fill="#f6e6b4" stroke="#d8c27e" stroke-width="3"/>
  <text x="32" y="42" font-family="Georgia, serif" font-size="36" font-weight="700"
        text-anchor="middle" fill="#b4451f">S</text>
  <text x="52" y="56" font-family="system-ui, sans-serif" font-size="12" font-weight="600"
        text-anchor="middle" fill="#1f2933">1</text>
  <circle cx="54" cy="10" r="9" fill="#e53e3e"/>
</svg>`;
    link.href = "data:image/svg+xml," + encodeURIComponent(svg);
  }
  document.head.appendChild(link);
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

// John Mode helper: valid 2-letter words containing the active rack letter.
function JohnHint({ letter }) {
  if (!letter) {
    return html`<p class="john-hint muted">Hover or select a rack tile to see its 2-letter words.</p>`;
  }
  const up = letter.toUpperCase();
  const words = TWO_LETTER_WORDS.filter((w) => w.includes(up));
  return html`<p class="john-hint">
    <span class="john-hint-label">2-letter words with ${up}:</span>${" "}
    ${words.length
      ? words.map((w) => html`<span class="john-word">${w}</span>`)
      : html`<span class="muted">none</span>`}
  </p>`;
}

function App({ gameId, initial }) {
  const [game, setGame] = useState(initial);
  const [pending, setPending] = useState([]);
  const [selected, setSelected] = useState(null);
  const [hoverLetter, setHoverLetter] = useState(null);
  const [mode, setMode] = useState("place");
  const [exchange, setExchange] = useState(() => new Set());
  const [error, setError] = useState(null);
  const [busy, setBusy] = useState(false);
  const [cursor, setCursor] = useState(null);
  const [blankPrompt, setBlankPrompt] = useState(null);
  const [rackOrder, setRackOrder] = useState(() =>
    (initial.your_rack || []).map((_, i) => i),
  );

  const [hintResult, setHintResult] = useState(null);
  const [hintsRemaining, setHintsRemaining] = useState(initial.hints_remaining || 0);
  const [hintBusy, setHintBusy] = useState(false);

  const yourTurn = isYourTurn(game);
  const seated = game.your_seat !== null && game.your_seat !== undefined;
  const hasOpenSeat = game.seats.some((s) => s.open);

  const lastPlaySet = useMemo(() => {
    const s = new Set();
    if (game.last_play) {
      for (const p of game.last_play) s.add(idx(p.row, p.col));
    }
    return s;
  }, [game]);

  useEffect(() => {
    setFavicon(yourTurn);
    return () => setFavicon(false);
  }, [yourTurn]);

  useEffect(() => {
    setHintsRemaining(game.hints_remaining || 0);
    setHintResult(null);
  }, [game.turn]);

  // Poll for opponent/bot moves while we are waiting.
  useEffect(() => {
    if (yourTurn || game.status === "Finished") return undefined;
    const timer = setInterval(async () => {
      try {
        const res = await fetch(`/games/${gameId}/state`);
        if (!res.ok) return;
        const next = await res.json();
        if (isYourTurn(next)) {
          const who = next.moves.length
            ? next.seats[next.moves[next.moves.length - 1].seat]?.name
            : null;
          notifyTurn(
            who ? `${who} just played` : "It's your turn in Screwball",
          );
        }
        setGame(next);
      } catch {
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

  // Inject turn indicator into nav bar on mobile
  const status = statusText(game);
  useEffect(() => {
    if (window.innerWidth > 480) return;
    const nav = document.querySelector(".nav");
    if (!nav) return;
    let el = nav.querySelector(".turn-indicator");
    if (!el) {
      el = document.createElement("span");
      el.className = "turn-indicator";
      nav.querySelector(".nav-links").before(el);
    }
    el.textContent = status;
    return () => el.remove();
  }, [status]);

  const usedRackIds = new Set(pending.map((p) => p.rackId));
  const rackTiles = game.your_rack || [];
  const order =
    rackOrder.length === rackTiles.length
      ? rackOrder
      : rackTiles.map((_, i) => i);
  // Keep placed tiles in the rack as inert placeholders so the remaining
  // letters never shift position mid-turn — you can tap-place rapidly without
  // chasing a moving target.
  const rack = order.map((id) => ({
    id,
    letter: rackTiles[id].letter,
    is_blank: rackTiles[id].is_blank,
    used: usedRackIds.has(id),
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
    // Tap-to-place: if cursor is set, tapping a rack tile places it directly
    if (cursor && mode === "place") {
      const target = firstEmptyFrom(cursor.row, cursor.col, cursor.dir, pending);
      if (!target) {
        setError("No room to place a tile that way.");
        return;
      }
      if (tile.is_blank) {
        setBlankPrompt({ row: target.row, col: target.col, rackId: tile.id });
        return;
      }
      const next = [
        ...pending,
        { row: target.row, col: target.col, letter: tile.letter, isBlank: false, rackId: tile.id },
      ];
      setPending(next);
      const after =
        cursor.dir === "down"
          ? { r: target.row + 1, c: target.col }
          : { r: target.row, c: target.col + 1 };
      const advanced = firstEmptyFrom(after.r, after.c, cursor.dir, next);
      setCursor(advanced ? { ...advanced, dir: cursor.dir } : cursor);
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
    const exact = rack.find((t) => !t.used && !t.is_blank && t.letter === letter);
    const blank = rack.find((t) => !t.used && t.is_blank);
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

  function placeTileOnBoard(tile, row, col) {
    if (!yourTurn || mode !== "place") return;
    if (game.board[idx(row, col)].letter) return;
    if (pending.some((p) => p.row === row && p.col === col)) return;
    if (tile.is_blank) {
      setBlankPrompt({ row, col, rackId: tile.id });
      return;
    }
    setPending([
      ...pending,
      { row, col, letter: tile.letter, isBlank: false, rackId: tile.id },
    ]);
    setSelected(null);
  }

  function dropTileOnBoard(tileId, row, col) {
    if (!yourTurn || mode !== "place") return;
    const tile = rackTiles[tileId];
    if (!tile) return;
    if (game.board[idx(row, col)].letter) return;
    if (pending.some((p) => p.row === row && p.col === col)) return;
    if (pending.some((p) => p.rackId === tileId)) return;
    if (tile.is_blank) {
      setBlankPrompt({ row, col, rackId: tileId });
      return;
    }
    setPending([
      ...pending,
      { row, col, letter: tile.letter, isBlank: tile.is_blank, rackId: tileId },
    ]);
    setSelected(null);
  }

  async function requestHint() {
    setHintBusy(true);
    try {
      const res = await fetch(`/games/${gameId}/hint`, { method: "POST" });
      const data = await res.json();
      if (!res.ok) {
        setError(data.error || "Could not get hint.");
        return;
      }
      setHintsRemaining(data.remaining);
      if (data.words && data.words.length) {
        setHintResult(`Try: ${data.words.join(", ")} (${data.score} pts)`);
      } else {
        setHintResult(data.message || "No plays available.");
      }
    } catch {
      setError("Network error — try again.");
    } finally {
      setHintBusy(false);
    }
  }

  async function postMove(body) {
    setBusy(true);
    setError(null);
    try {
      requestNotificationPermission();
      const res = await fetch(`/games/${gameId}/move`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      let data = null;
      try {
        data = await res.json();
      } catch {
        data = null;
      }
      if (!res.ok) {
        if (res.status === 401) {
          setError(
            (data && data.error) ||
              "Your session expired — reload the page and sign in again.",
          );
        } else {
          setError((data && data.error) || "Move rejected.");
        }
        return;
      }
      if (!data) {
        setError("Unexpected response — try again.");
        return;
      }
      setGame(data);
      reset();
    } catch {
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
          lastPlaySet=${lastPlaySet}
          onCellClick=${onCellClick}
          onPendingClick=${recallTile}
          onDropTile=${dropTileOnBoard}
        />
        ${!seated && hasOpenSeat ? html`<${JoinForm} gameId=${gameId} />` : null}
      </div>
      ${seated && !finished
        ? html`<div class="rack-area">
            <${Rack}
              tiles=${rack}
              selected=${selected}
              mode=${mode}
              exchange=${exchange}
              onSelect=${selectTile}
              onReorder=${reorderRack}
              onPlaceOnBoard=${placeTileOnBoard}
              onHover=${game.john_mode ? setHoverLetter : null}
            />
            ${game.john_mode
              ? html`<${JohnHint}
                  letter=${hoverLetter ||
                  (selected != null && rackTiles[selected] && !rackTiles[selected].is_blank
                    ? rackTiles[selected].letter
                    : null)}
                />`
              : null}
            ${error ? html`<p class="move-error">${error}</p>` : null}
            <div class="controls">
              ${mode === "place"
                ? html`<button
                      type="button"
                      class="button"
                      disabled=${controlsDisabled}
                      onClick=${submitPlay}
                    >
                      Play word${pending.length ? ` (${previewScore(game, pending)})` : ""}
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
            ${game.hints_allowed > 0
              ? html`<div>
                  <button type="button" class="hint-btn"
                    disabled=${!yourTurn || hintBusy || hintsRemaining <= 0}
                    onClick=${requestHint}>
                    Hint (${hintsRemaining} left)
                  </button>
                  ${hintResult ? html`<p class="hint-result">${hintResult}</p>` : null}
                </div>`
              : null}
          </div>`
        : null}
      <aside class="sidebar">
        <div class="game-badges">
          ${game.john_mode ? html`<span class="game-badge">John Mode</span>` : null}
          ${game.grandpa_mode ? html`<span class="game-badge">Grandpa Mode</span>` : null}
          ${game.hints_allowed > 0 ? html`<span class="game-badge">${game.hints_allowed} hint${game.hints_allowed > 1 ? "s" : ""}/player</span>` : null}
        </div>
        <${Scoreboard} game=${game} />
        <p class="muted">Tiles in bag: ${game.bag_count}</p>
        <${MoveLog} game=${game} />
        <${OtherGames} gameId=${gameId} />
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

  // Prevent pull-to-refresh and rubber-band scrolling on iOS (mobile only)
  document.addEventListener(
    "touchmove",
    (e) => {
      if (window.innerWidth > 480) return;
      if (e.target.closest(".modal-backdrop")) return;
      if (e.target.closest(".sidebar")) return;
      // Allow pinch-to-zoom on the board
      if (e.touches.length >= 2 && e.target.closest(".board")) return;
      e.preventDefault();
    },
    { passive: false },
  );
}

boot();
