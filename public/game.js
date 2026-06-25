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

function Cell({ square, pending, row, col, onClick }) {
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
  const cls = premium === "none" ? "cell" : `cell premium-${premium}`;
  const star = row === CENTER && col === CENTER ? "★" : PREMIUM_LABEL[premium];
  return html`<div class=${cls} onClick=${onClick}>
    <span class="premium-label">${star}</span>
  </div>`;
}

function Board({ game, pending, onCellClick, onPendingClick }) {
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
          onClick=${() =>
            place ? onPendingClick(place) : onCellClick(row, col)}
        />`,
      );
    }
  }
  return html`<div class="board" role="grid">${cells}</div>`;
}

function Rack({ tiles, selected, mode, exchange, onSelect }) {
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
      } else {
        detail = "passed";
      }
      return html`<li key=${n}><strong>${name}</strong>: ${detail}</li>`;
    })}
  </ul>`;
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
  const gameRef = useRef(game);
  gameRef.current = game;

  const yourTurn = isYourTurn(game);

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

  const usedRackIds = new Set(pending.map((p) => p.rackId));
  const rack = (game.your_rack || [])
    .map((tile, id) => ({ id, letter: tile.letter, is_blank: tile.is_blank }))
    .filter((tile) => !usedRackIds.has(tile.id));

  function reset() {
    setPending([]);
    setSelected(null);
    setMode("place");
    setExchange(new Set());
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

  function placeAt(row, col) {
    if (!yourTurn || mode === "exchange" || selected === null) return;
    if (game.board[idx(row, col)].letter) return;
    const tile = (game.your_rack || [])[selected];
    if (!tile) return;
    let letter = tile.letter;
    let isBlank = tile.is_blank;
    if (isBlank) {
      const choice = window.prompt("Letter for blank tile (A–Z):", "");
      if (!choice) return;
      const up = choice.trim().toUpperCase();
      if (!/^[A-Z]$/.test(up)) {
        setError("Blank tile needs a single letter A–Z.");
        return;
      }
      letter = up;
    }
    setPending([...pending, { row, col, letter, isBlank, rackId: selected }]);
    setSelected(null);
  }

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
      const tile = game.your_rack[id];
      return tile.is_blank ? "?" : tile.letter;
    });
    postMove({ kind: "exchange", tiles });
  }

  const controlsDisabled = !yourTurn || busy;

  return html`<div class="game">
    <h1 class="status">${statusText(game)}</h1>
    <div class="game-layout">
      <div class="board-wrap">
        <${Board}
          game=${game}
          pending=${pending}
          onCellClick=${placeAt}
          onPendingClick=${recallTile}
        />
        <div class="rack-area">
          <${Rack}
            tiles=${rack}
            selected=${selected}
            mode=${mode}
            exchange=${exchange}
            onSelect=${selectTile}
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
                    onClick=${() => setPending([])}
                  >
                    Recall
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
                Tap a rack tile, then tap a board square to place it. Tap a
                placed tile to take it back.
              </p>`
            : null}
        </div>
      </div>
      <aside class="sidebar">
        <${Scoreboard} game=${game} />
        <p class="muted">Tiles in bag: ${game.bag_count}</p>
        <${MoveLog} game=${game} />
      </aside>
    </div>
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
