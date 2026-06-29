import {
  h,
  html,
  render,
  useState,
  useEffect,
  useLayoutEffect,
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

const GRANDPA_TWO_LETTER_WORDS = (() => {
  const el = document.getElementById("grandpa-two-letter-words");
  try {
    return new Set(el ? JSON.parse(el.textContent) : []);
  } catch {
    return new Set();
  }
})();

function pointsFor(letter, isBlank) {
  return isBlank ? 0 : POINTS[letter] || 0;
}

function idx(row, col) {
  return row * SIZE + col;
}

function rackOrderAfterInsertion(base, fromId, target) {
  const from = base.indexOf(fromId);
  const to = base.indexOf(target.id);
  if (from === -1 || to === -1) return null;
  let insertIndex = to + (target.side === "after" ? 1 : 0);
  const next = base.slice();
  const [moved] = next.splice(from, 1);
  if (from < insertIndex) insertIndex -= 1;
  next.splice(insertIndex, 0, moved);
  return next.every((id, i) => id === base[i]) ? null : next;
}

function clearRackInsertionMarkers(root = document) {
  for (const el of root.querySelectorAll(".rack-insert-before,.rack-insert-after")) {
    el.classList.remove("rack-insert-before", "rack-insert-after");
  }
}

let activeDragPreview = null;

function setActiveDragPreview(letter, points, isBlank = false) {
  activeDragPreview = {
    letter: letter || "",
    points: String(points ?? ""),
    isBlank: Boolean(isBlank),
  };
}

function clearActiveDragPreview() {
  activeDragPreview = null;
  clearBoardDropGhost();
}

function clearBoardDropGhost() {
  for (const el of document.querySelectorAll(".board-drop-ghost,.cell.drag-over")) {
    el.classList.remove("board-drop-ghost", "drag-over");
    delete el.dataset.dropLetter;
    delete el.dataset.dropPoints;
    delete el.dataset.dropBlank;
  }
}

function showBoardDropGhost(cell, preview = activeDragPreview) {
  clearBoardDropGhost();
  if (!cell || !preview) return;
  cell.classList.add("drag-over", "board-drop-ghost");
  cell.dataset.dropLetter = preview.letter;
  cell.dataset.dropPoints = preview.points;
  cell.dataset.dropBlank = preview.isBlank ? "true" : "false";
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
function Tile({ letter, isBlank, points, pending, onClick }) {
  const cls = ["tile-face", isBlank ? "tile-blank" : "", pending ? "pending" : ""]
    .filter(Boolean)
    .join(" ");
  return html`<span class=${cls} onClick=${onClick}>
    <span class="tile-letter">${letter}</span>
    <span class="tile-points">${points}</span>
  </span>`;
}

function Cell({
  square,
  pending,
  row,
  col,
  cursor,
  lastPlay,
  onClick,
  onDragOver,
  onDragLeave,
  onDrop,
}) {
  if (square.letter) {
    const cls = lastPlay ? "cell tile last-play" : "cell tile";
    return html`<div class=${cls} data-row=${row} data-col=${col}>
      <${Tile}
        letter=${square.letter}
        isBlank=${square.is_blank}
        points=${pointsFor(square.letter, square.is_blank)}
      />
    </div>`;
  }
  if (pending) {
    return html`<div class="cell tile pending-cell"
      data-row=${row}
      data-col=${col}
      data-rackid=${pending.rackId}
      draggable=${true}
      onDragStart=${(e) => {
        setActiveDragPreview(
          pending.letter,
          pointsFor(pending.letter, pending.isBlank),
          pending.isBlank,
        );
        e.dataTransfer.setData("text/plain", "pending:" + pending.rackId);
        e.dataTransfer.effectAllowed = "move";
      }}
      onDragEnd=${() => {
        setRackRecallActive(false);
        clearActiveDragPreview();
      }}
      onClick=${onClick}
    >
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
    onDragOver=${onDragOver} onDragLeave=${onDragLeave} onDrop=${onDrop}>
    ${isCursor
      ? html`<span class="cursor-arrow">${cursor.dir === "down" ? "↓" : "→"}</span>`
      : html`<span class="premium-label">${star}</span>`}
  </div>`;
}

function Board({
  game,
  pending,
  cursor,
  lastPlaySet,
  onCellClick,
  onPendingClick,
  onDropTile,
  onMovePending,
  onRecallPending,
}) {
  const byPos = new Map(pending.map((p) => [idx(p.row, p.col), p]));
  const boardRef = useRef(null);
  const pinchState = useRef(null);
  const scaleRef = useRef(1);
  const panRef = useRef({ x: 0, y: 0 });
  const panStartRef = useRef(null);
  const pendingDrag = useRef(null);

  function applyTransform() {
    if (!boardRef.current) return;
    const s = scaleRef.current;
    const { x, y } = panRef.current;
    boardRef.current.style.transform = s === 1 && x === 0 && y === 0
      ? "" : `translate(${x}px, ${y}px) scale(${s})`;
  }

  function handleBoardTouchStart(e) {
    // Start dragging an already-placed (pending) tile to another square (205).
    if (e.touches.length === 1) {
      const tileEl = e.target.closest(".pending-cell[data-rackid]");
      if (tileEl) {
        const rackId = Number(tileEl.dataset.rackid);
        const dragPending = pending.find((p) => p.rackId === rackId);
        const letterEl = tileEl.querySelector(".tile-letter");
        pendingDrag.current = {
          rackId,
          startX: e.touches[0].clientX,
          startY: e.touches[0].clientY,
          dragging: false,
          ghost: null,
          boardDropLiftY: 0,
          letter: dragPending ? dragPending.letter : (letterEl ? letterEl.textContent : ""),
          points: dragPending ? pointsFor(dragPending.letter, dragPending.isBlank) : "",
          isBlank: dragPending ? dragPending.isBlank : false,
          highlightedCell: null,
          currentBoardCell: null,
        };
        return;
      }
    }
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
    if (pendingDrag.current && e.touches.length === 1) {
      const touch = e.touches[0];
      const pd = pendingDrag.current;
      if (
        !pd.dragging &&
        Math.abs(touch.clientX - pd.startX) + Math.abs(touch.clientY - pd.startY) > 8
      ) {
        pd.dragging = true;
        const ghost = document.createElement("div");
        ghost.className = "rack-tile-ghost";
        ghost.textContent = pd.letter;
        positionDragGhost(ghost, touch.clientX, touch.clientY);
        document.body.appendChild(ghost);
        pd.boardDropLiftY = boardDropLiftPx();
        pd.ghost = ghost;
      }
      if (pd.dragging) {
        e.preventDefault();
        if (pd.ghost) {
          positionDragGhost(pd.ghost, touch.clientX, touch.clientY);
        }
        if (pd.highlightedCell) {
          clearBoardDropGhost();
          pd.highlightedCell = null;
        }
        const target = boardDropPoint(touch, pd.boardDropLiftY);
        const overRack =
          isPointOverRack(touch.clientX, touch.clientY) ||
          isPointOverRack(target.x, target.y);
        setRackRecallActive(overRack);
        if (overRack) {
          pd.currentBoardCell = null;
          clearBoardDropGhost();
          return;
        }
        const cell = cellAtPoint(target.x, target.y);
        if (cell) {
          showBoardDropGhost(cell, {
            letter: pd.letter,
            points: String(pd.points),
            isBlank: pd.isBlank,
          });
          pd.highlightedCell = cell;
          pd.currentBoardCell = cell;
        } else {
          pd.currentBoardCell = null;
          clearBoardDropGhost();
        }
      }
      return;
    }
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
    if (pendingDrag.current) {
      const pd = pendingDrag.current;
      const previewCell = pd.currentBoardCell;
      if (pd.ghost) pd.ghost.remove();
      if (pd.highlightedCell) clearBoardDropGhost();
      setRackRecallActive(false);
      if (pd.dragging) {
        const lastTouch = e.changedTouches[0];
        const target = boardDropPoint(lastTouch, pd.boardDropLiftY);
        if (
          (isPointOverRack(lastTouch.clientX, lastTouch.clientY) ||
            isPointOverRack(target.x, target.y)) &&
          onRecallPending
        ) {
          onRecallPending(pd.rackId);
        } else {
          const cell = previewCell || cellAtPoint(target.x, target.y);
          if (cell && onMovePending) {
            onMovePending(pd.rackId, Number(cell.dataset.row), Number(cell.dataset.col));
          }
        }
      }
      pendingDrag.current = null;
      return;
    }
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

  function handleBoardTouchCancel() {
    if (pendingDrag.current) {
      if (pendingDrag.current.ghost) pendingDrag.current.ghost.remove();
      if (pendingDrag.current.highlightedCell) {
        clearBoardDropGhost();
      }
      pendingDrag.current = null;
    }
    setRackRecallActive(false);
  }

  useEffect(() => {
    function onDocumentTouchMove(e) {
      if (pendingDrag.current) handleBoardTouchMove(e);
    }
    function onDocumentTouchEnd(e) {
      if (pendingDrag.current) handleBoardTouchEnd(e);
    }
    function onDocumentTouchCancel() {
      if (pendingDrag.current) handleBoardTouchCancel();
    }
    document.addEventListener("touchmove", onDocumentTouchMove, { passive: false });
    document.addEventListener("touchend", onDocumentTouchEnd, { passive: false });
    document.addEventListener("touchcancel", onDocumentTouchCancel, { passive: false });
    return () => {
      document.removeEventListener("touchmove", onDocumentTouchMove);
      document.removeEventListener("touchend", onDocumentTouchEnd);
      document.removeEventListener("touchcancel", onDocumentTouchCancel);
    };
  });

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
          onDragOver=${isEmpty ? (e) => {
            e.preventDefault();
            showBoardDropGhost(e.currentTarget);
          } : null}
          onDragLeave=${isEmpty ? (e) => {
            if (!e.currentTarget.contains(e.relatedTarget)) clearBoardDropGhost();
          } : null}
          onDrop=${isEmpty ? (e) => {
            e.preventDefault();
            clearActiveDragPreview();
            const data = e.dataTransfer.getData("text/plain");
            if (data.startsWith("pending:")) {
              if (onMovePending) onMovePending(Number(data.slice(8)), row, col);
            } else if (onDropTile) {
              onDropTile(Number(data), row, col);
            }
          } : null}
        />`,
      );
    }
  }
  return html`<div class="board" role="grid" ref=${boardRef}
    onDragOver=${(e) => {
      clearRackInsertionMarkers();
      if (!e.target.closest?.(".cell:not(.tile)")) clearBoardDropGhost();
    }}
    onDragLeave=${(e) => {
      if (!e.currentTarget.contains(e.relatedTarget)) clearBoardDropGhost();
    }}
    onDrop=${() => {
      clearRackInsertionMarkers();
      clearBoardDropGhost();
    }}
    onTouchStart=${(e) => { handleDoubleTap(e); handleBoardTouchStart(e); }}
    onTouchMove=${handleBoardTouchMove}
    onTouchEnd=${handleBoardTouchEnd}
    onTouchCancel=${handleBoardTouchCancel}
  >${cells}</div>`;
}

// Find a board cell at/near a screen point. Probes a small radius so a touch
// that lands slightly off a square still resolves to it (forgiving drop).
function boardCellAtPoint(x, y, emptyOnly = false) {
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
      (!emptyOnly || !cell.classList.contains("tile"))
    ) {
      return cell;
    }
  }
  return null;
}

function cellAtPoint(x, y) {
  return boardCellAtPoint(x, y, true);
}

function isPointOverBoard(x, y) {
  const board = document.querySelector(".board");
  if (!board) return false;
  const rect = board.getBoundingClientRect();
  return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
}

function isPointOverRack(x, y) {
  const rack = document.querySelector(".rack");
  if (!rack) return false;
  const rect = rack.getBoundingClientRect();
  return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
}

function setRackRecallActive(active) {
  const rack = document.querySelector(".rack");
  if (rack) rack.classList.toggle("rack-recall-over", active);
}

function pendingRackIdFromDrop(e) {
  const data = e.dataTransfer ? e.dataTransfer.getData("text/plain") : "";
  if (!data.startsWith("pending:")) return null;
  const rackId = Number(data.slice(8));
  return Number.isFinite(rackId) ? rackId : null;
}

function handleRackDragLeave(e) {
  if (!e.currentTarget.contains(e.relatedTarget)) {
    e.currentTarget.classList.remove("rack-recall-over");
  }
}

function dragGhostLiftPx() {
  const fontSize = Number.parseFloat(
    getComputedStyle(document.documentElement).fontSize,
  );
  return 2.4 * (Number.isFinite(fontSize) ? fontSize : 16);
}

function boardDropLiftPx() {
  return dragGhostLiftPx() * 0.4;
}

function boardDropPoint(touch, liftY) {
  return {
    x: touch.clientX,
    y: touch.clientY - liftY,
  };
}

function positionDragGhost(ghost, x, y) {
  ghost.style.setProperty("--drag-x", `${x}px`);
  ghost.style.setProperty("--drag-y", `${y}px`);
}

function Rack({
  tiles,
  selected,
  mode,
  exchange,
  onSelect,
  onReorder,
  onPlaceOnBoard,
  onRecallPending,
  onBackspace,
  showBackspace,
  onHover,
}) {
  const dragId = useRef(null);
  const dragSourceEl = useRef(null);
  const dragRackTarget = useRef(null);
  const touchState = useRef(null);
  const rackRef = useRef(null);
  const flipPositions = useRef(new Map());

  // FLIP: smoothly slide rack tiles whenever their order changes (203). The
  // tile under the finger is excluded — it tracks the drag ghost instead.
  useLayoutEffect(() => {
    const rack = rackRef.current;
    if (!rack) return;
    const prev = flipPositions.current;
    const next = new Map();
    const dragging = touchState.current ? touchState.current.originalId : null;
    for (const el of rack.querySelectorAll(".rack-tile[data-tile-id]")) {
      const id = Number(el.dataset.tileId);
      const left = el.getBoundingClientRect().left;
      next.set(id, left);
      const old = prev.get(id);
      if (old != null && Math.abs(old - left) > 0.5 && id !== dragging) {
        const dx = old - left;
        el.style.transition = "none";
        el.style.transform = `translateX(${dx}px)`;
        requestAnimationFrame(() => {
          el.style.transition = "transform 0.15s ease";
          el.style.transform = "";
        });
      }
    }
    flipPositions.current = next;
  });

  // Which rack tile the visible dragged tile is reordering toward: the tile
  // whose horizontal centre is nearest. The lower band extends to the bottom of
  // the viewport so dragging below the rack still moves the insertion marker.
  // Moving the visible tile above the rack exits reorder mode for board drops.
  function rackReorderTarget(x, y, draggedId) {
    const rack = rackRef.current;
    if (!rack) return null;
    const rect = rack.getBoundingClientRect();
    const PAD_X = 40;
    const PAD_TOP = 0;
    const PAD_BOTTOM = Math.max(120, window.innerHeight - rect.bottom);
    if (x < rect.left - PAD_X || x > rect.right + PAD_X) return null;
    if (y < rect.top - PAD_TOP || y > rect.bottom + PAD_BOTTOM) return null;
    let bestId = null;
    let bestSide = "before";
    let bestDist = Infinity;
    for (const el of rack.querySelectorAll(".rack-tile[data-tile-id]")) {
      const id = Number(el.dataset.tileId);
      if (id === draggedId) continue;
      const r = el.getBoundingClientRect();
      const center = r.left + r.width / 2;
      const dist = Math.abs(center - x);
      if (dist < bestDist) {
        bestDist = dist;
        bestId = id;
        bestSide = x < center ? "before" : "after";
      }
    }
    return bestId == null ? null : { id: bestId, side: bestSide };
  }

  function tileInsertionTarget(tileId, x) {
    const rack = rackRef.current;
    if (!rack) return { id: tileId, side: "before" };
    const el = rack.querySelector(`.rack-tile[data-tile-id="${tileId}"]`);
    if (!el) return { id: tileId, side: "before" };
    const rect = el.getBoundingClientRect();
    return {
      id: tileId,
      side: x < rect.left + rect.width / 2 ? "before" : "after",
    };
  }

  function clearRackInsertionMarker() {
    const rack = rackRef.current;
    if (!rack) return;
    clearRackInsertionMarkers(rack);
  }

  function showRackInsertionMarker(target) {
    clearRackInsertionMarker();
    const rack = rackRef.current;
    if (!rack || !target) return;
    const el = rack.querySelector(`.rack-tile[data-tile-id="${target.id}"]`);
    if (!el) return;
    el.classList.add(target.side === "before" ? "rack-insert-before" : "rack-insert-after");
  }

  function shouldReorderToward(draggedId, target) {
    return rackOrderAfterInsertion(
      tiles.map((tile) => tile.id),
      draggedId,
      target,
    ) !== null;
  }

  function desktopRackTarget(e) {
    if (dragId.current === null || isPointOverBoard(e.clientX, e.clientY)) {
      return null;
    }
    const target = rackReorderTarget(e.clientX, e.clientY, dragId.current);
    return target && shouldReorderToward(dragId.current, target) ? target : null;
  }

  function previewDesktopRackTarget(e) {
    const target = desktopRackTarget(e);
    dragRackTarget.current = target;
    if (target) {
      clearBoardDropGhost();
      showRackInsertionMarker(target);
    } else {
      clearRackInsertionMarker();
    }
    return target;
  }

  function finishDesktopDrag() {
    if (dragSourceEl.current) {
      dragSourceEl.current.classList.remove("dragging");
    }
    clearRackInsertionMarker();
    clearActiveDragPreview();
    dragId.current = null;
    dragSourceEl.current = null;
    dragRackTarget.current = null;
  }

  function commitDesktopRackTarget(target = dragRackTarget.current) {
    if (dragId.current === null || !target) return false;
    if (!shouldReorderToward(dragId.current, target)) return false;
    onReorder(dragId.current, target);
    return true;
  }

  function handleDesktopDragEnd() {
    commitDesktopRackTarget();
    finishDesktopDrag();
  }

  function handleRackDragOver(e) {
    if (dragId.current !== null) return;
    e.preventDefault();
    clearBoardDropGhost();
    clearRackInsertionMarker();
    e.currentTarget.classList.add("rack-recall-over");
    if (e.dataTransfer) {
      e.dataTransfer.dropEffect = "move";
    }
  }

  function handleRackDrop(e) {
    const rackId = pendingRackIdFromDrop(e);
    e.currentTarget.classList.remove("rack-recall-over");
    if (rackId == null || !onRecallPending) return;
    e.preventDefault();
    onRecallPending(rackId);
  }

  function touchDropTarget(touch, state) {
    const rack = rackRef.current;
    const rackRect = rack ? rack.getBoundingClientRect() : null;
    const point = boardDropPoint(touch, state.boardDropLiftY);
    const boardCell = cellAtPoint(point.x, point.y);
    if (
      boardCell ||
      isPointOverBoard(point.x, point.y) ||
      isPointOverBoard(touch.clientX, touch.clientY)
    ) {
      return { kind: "board", cell: boardCell };
    }
    const rackPoint = rackRect && touch.clientY >= rackRect.top
      ? { x: touch.clientX, y: touch.clientY }
      : point;
    const target = rackReorderTarget(rackPoint.x, rackPoint.y, state.originalId);
    if (target && shouldReorderToward(state.originalId, target)) {
      return { kind: "rack", target };
    }
    return { kind: "none" };
  }

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
      boardDropLiftY: 0,
      currentRackTarget: null,
      currentBoardCell: null,
      sourceEl: e.currentTarget,
      letter: tile.is_blank ? "" : tile.letter,
      points: pointsFor(tile.letter, tile.is_blank),
      isBlank: tile.is_blank,
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
      positionDragGhost(ghost, touch.clientX, touch.clientY);
      document.body.appendChild(ghost);
      touchState.current.boardDropLiftY = boardDropLiftPx();
      touchState.current.ghost = ghost;
    }
    if (touchState.current.dragging) {
      e.preventDefault();
      const ghost = touchState.current.ghost;
      if (ghost) {
        positionDragGhost(ghost, touch.clientX, touch.clientY);
      }
      // Clear previous highlight
      if (touchState.current.highlightedCell) {
        clearBoardDropGhost();
        touchState.current.highlightedCell = null;
      }
      const dropTarget = touchDropTarget(touch, touchState.current);
      if (dropTarget.kind === "rack") {
        touchState.current.currentRackTarget = dropTarget.target;
        touchState.current.currentBoardCell = null;
        clearBoardDropGhost();
        showRackInsertionMarker(dropTarget.target);
      } else {
        touchState.current.currentRackTarget = null;
        clearRackInsertionMarker();
        if (dropTarget.kind === "board" && dropTarget.cell) {
          showBoardDropGhost(dropTarget.cell, {
            letter: touchState.current.letter,
            points: String(touchState.current.points),
            isBlank: touchState.current.isBlank,
          });
          touchState.current.highlightedCell = dropTarget.cell;
          touchState.current.currentBoardCell = dropTarget.cell;
        } else {
          touchState.current.currentBoardCell = null;
          clearBoardDropGhost();
        }
      }
    }
  }

  function clearTouchVisuals(state) {
    if (state.ghost) {
      state.ghost.remove();
    }
    if (state.sourceEl) {
      state.sourceEl.classList.remove("dragging");
    }
    clearRackInsertionMarker();
    if (state.highlightedCell) {
      clearBoardDropGhost();
    }
  }

  function handleTouchEnd(e) {
    const state = touchState.current;
    if (state) {
      clearTouchVisuals(state);
      if (state.dragging) {
        // If the insertion marker is visible, commit that exact target.
        // Otherwise drop onto the nearest board cell (forgiving of a near miss).
        const lastTouch = e.changedTouches[0];
        const rackTarget = state.currentRackTarget;
        if (rackTarget) {
          onReorder(state.originalId, rackTarget);
        } else {
          const dropTarget = touchDropTarget(lastTouch, state);
          const boardCell = state.currentBoardCell ||
            (dropTarget.kind === "board" ? dropTarget.cell : null);
          if (boardCell) {
            const row = Number(boardCell.dataset.row);
            const col = Number(boardCell.dataset.col);
            const tile = tiles.find((t) => t.id === state.originalId);
            if (tile && onPlaceOnBoard) {
              onPlaceOnBoard(tile, row, col);
            }
          }
        }
      } else {
        e.preventDefault();
        const tile = tiles.find((t) => t.id === state.id);
        if (tile) onSelect(tile);
      }
    }
    touchState.current = null;
  }

  function handleTouchCancel() {
    const state = touchState.current;
    if (!state) return;
    clearTouchVisuals(state);
    touchState.current = null;
  }

  useEffect(() => {
    function onDocumentTouchMove(e) {
      if (touchState.current) handleTouchMove(e);
    }
    function onDocumentTouchEnd(e) {
      if (touchState.current) handleTouchEnd(e);
    }
    function onDocumentTouchCancel() {
      if (touchState.current) handleTouchCancel();
    }
    document.addEventListener("touchmove", onDocumentTouchMove, { passive: false });
    document.addEventListener("touchend", onDocumentTouchEnd, { passive: false });
    document.addEventListener("touchcancel", onDocumentTouchCancel, { passive: false });
    return () => {
      document.removeEventListener("touchmove", onDocumentTouchMove);
      document.removeEventListener("touchend", onDocumentTouchEnd);
      document.removeEventListener("touchcancel", onDocumentTouchCancel);
    };
  });

  useEffect(() => {
    function onDocumentDragOver(e) {
      if (dragId.current === null) return;
      const target = previewDesktopRackTarget(e);
      if (!target) return;
      e.preventDefault();
      if (e.dataTransfer) {
        e.dataTransfer.dropEffect = "move";
      }
    }
    function onDocumentDrop(e) {
      if (dragId.current === null) return;
      const droppedOnRackTile = e.target.closest?.(".rack-tile[data-tile-id]");
      if (droppedOnRackTile) return;
      if (isPointOverBoard(e.clientX, e.clientY)) {
        dragRackTarget.current = null;
        return;
      }
      const target = desktopRackTarget(e) || dragRackTarget.current;
      if (!target) return;
      e.preventDefault();
      commitDesktopRackTarget(target);
      finishDesktopDrag();
    }
    document.addEventListener("dragover", onDocumentDragOver);
    document.addEventListener("drop", onDocumentDrop);
    return () => {
      document.removeEventListener("dragover", onDocumentDragOver);
      document.removeEventListener("drop", onDocumentDrop);
    };
  });

  return html`<div class="rack" ref=${rackRef}
    onDragOver=${handleRackDragOver}
    onDragLeave=${handleRackDragLeave}
    onDrop=${handleRackDrop}
    onMouseLeave=${() => onHover && onHover(null)}>
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
      return html`<button type="button"
        key=${tile.id}
        class=${cls}
        data-tile-id=${tile.id}
        draggable=${true}
        onDragStart=${(e) => {
          dragId.current = tile.id;
          dragSourceEl.current = e.currentTarget;
          dragRackTarget.current = null;
          e.currentTarget.classList.add("dragging");
          setActiveDragPreview(
            tile.is_blank ? "" : tile.letter,
            pointsFor(tile.letter, tile.is_blank),
            tile.is_blank,
          );
          e.dataTransfer.setData("text/plain", String(tile.id));
          e.dataTransfer.effectAllowed = "move";
        }}
        onDragOver=${(e) => {
          if (dragId.current !== null) {
            const target = previewDesktopRackTarget(e);
            if (target) {
              e.preventDefault();
            }
          }
        }}
        onDrop=${(e) => {
          const pendingRackId = pendingRackIdFromDrop(e);
          if (pendingRackId != null && onRecallPending) {
            e.preventDefault();
            e.stopPropagation();
            e.currentTarget.classList.remove("rack-recall-over");
            setRackRecallActive(false);
            onRecallPending(pendingRackId);
            return;
          }
          e.preventDefault();
          if (dragId.current !== null && dragId.current !== tile.id) {
            const target = tileInsertionTarget(tile.id, e.clientX);
            if (shouldReorderToward(dragId.current, target)) {
              onReorder(dragId.current, target);
            }
          }
          finishDesktopDrag();
        }}
        onDragEnd=${handleDesktopDragEnd}
        onTouchStart=${(e) => handleTouchStart(e, tile)}
        onClick=${() => onSelect(tile)}
        onMouseEnter=${() => onHover && onHover(tile.is_blank ? null : tile.letter)}
        onFocus=${() => onHover && onHover(tile.is_blank ? null : tile.letter)}
        onBlur=${() => onHover && onHover(null)}
      >
        <span class="tile-letter">${tile.is_blank ? " " : tile.letter}</span>
        <span class="tile-points">${pointsFor(tile.letter, tile.is_blank)}</span>
      </button>`;
    })}
    ${showBackspace
      ? html`<button type="button"
          class="rack-tile rack-backspace-tile"
          aria-label="Backspace"
          onClick=${onBackspace}
        >
          <span class="tile-letter">←</span>
        </button>`
      : null}
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
            ${seat.hints_unlimited
              ? html`<span class="hint-count" title="unlimited hints">💡∞</span>`
              : seat.hints_remaining != null
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

// Look up a word's definition from our server, which caches results and falls
// back from dictionaryapi.dev to Wiktionary. Returns { pos, text } or null.
async function fetchDefinition(word) {
  const res = await fetch(
    `/api/define/${encodeURIComponent(word.toLowerCase())}`,
  ).catch(() => null);
  if (!res || !res.ok) return null;
  return res.json();
}

function MoveLog({ game }) {
  // word (uppercase) -> { open, loading, def } cached across re-renders.
  const [defs, setDefs] = useState({});

  function patch(key, fields) {
    setDefs((d) => ({ ...d, [key]: { ...d[key], ...fields } }));
  }

  async function toggleDef(word) {
    const key = word.toUpperCase();
    const cur = defs[key];
    if (cur && cur.open) {
      patch(key, { open: false });
      return;
    }
    if (cur && "def" in cur) {
      patch(key, { open: true });
      return;
    }
    patch(key, { open: true, loading: true });
    try {
      const def = await fetchDefinition(word);
      patch(key, { open: true, loading: false, def });
    } catch {
      patch(key, { open: true, loading: false, def: null });
    }
  }

  if (!game.moves.length) {
    return html`<p class="muted">No moves yet.</p>`;
  }
  const recent = game.moves.slice(-10).toReversed();
  return html`<ul class="move-log">
    ${recent.map((mv, n) => {
      const name = game.seats[mv.seat] ? game.seats[mv.seat].name : "?";
      if (mv.kind === "play") {
        return html`<li key=${n}>
          <strong>${name}</strong>:${" "}
          ${mv.words.map(
            (w, i) => html`${i ? ", " : ""}<button
                type="button"
                class="word-def"
                title="Show definition"
                onClick=${() => toggleDef(w)}
              >${w}</button>`,
          )}
          ${` (+${mv.points})`}
          ${mv.words
            .filter((w) => defs[w.toUpperCase()] && defs[w.toUpperCase()].open)
            .map((w) => {
              const d = defs[w.toUpperCase()];
              return html`<div class="word-definition" key=${w}>
                <strong>${w}</strong>${" "}
                ${d.loading
                  ? html`<span class="muted">…</span>`
                  : d.def
                    ? html`<span class="muted">(${d.def.pos})</span> ${d.def.text}`
                    : html`<span class="muted">no definition found</span>`}
              </div>`;
            })}
        </li>`;
      }
      let detail;
      if (mv.kind === "exchange") {
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
  const ranked = [];
  for (const seat of game.seats) {
    const insertAt = ranked.findIndex((other) => seat.score > other.score);
    if (insertAt === -1) ranked.push(seat);
    else ranked.splice(insertAt, 0, seat);
  }
  const winners = new Set(game.winners);
  return h("div", { class: "results card" }, [
    html`<h2>Game over</h2>`,
    h(
      "ol",
      { class: "results-list" },
      ranked.map((seat) =>
        h(
          "li",
          {
            key: seat.index,
            class: winners.has(seat.index) ? "winner" : "",
          },
          [
            h("span", { class: "results-name" }, [
              seat.name,
              winners.has(seat.index) ? html`<span class="badge">winner</span>` : null,
            ]),
            h("span", { class: "results-score" }, seat.score),
          ],
        ),
      ),
    ),
    html`<a class="button" href="/">New game</a>`,
  ]);
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
          (l) => html`<button type="button"
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
          const others = data.filter((g) => g.id !== gameId && g.status !== "finished");
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
          setTurnAffordanceSource("other-games", nowTurn.size);
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
      setTurnAffordanceSource("other-games", 0);
      clearInterval(timer);
    };
  }, []);

  if (!games) return null;
  const others = games.filter((g) => g.id !== gameId && g.status !== "finished");
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

function notificationSupport() {
  if (
    !("Notification" in window) ||
    !("serviceWorker" in navigator) ||
    !("PushManager" in window)
  ) {
    return "unsupported";
  }
  return Notification.permission;
}

function urlBase64ToUint8Array(value) {
  const padding = "=".repeat((4 - (value.length % 4)) % 4);
  const base64 = (value + padding).replace(/-/g, "+").replace(/_/g, "/");
  const raw = window.atob(base64);
  const output = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) {
    output[i] = raw.charCodeAt(i);
  }
  return output;
}

let pushSetupPromise = null;

async function ensurePushNotifications({ prompt = false } = {}) {
  if (notificationSupport() === "unsupported") return "unsupported";
  let permission = Notification.permission;
  if (permission === "default" && prompt) {
    permission = await Notification.requestPermission();
  }
  if (permission !== "granted") return permission;
  if (pushSetupPromise) return pushSetupPromise;

  pushSetupPromise = (async () => {
    const keyRes = await fetch("/api/push/vapid-public-key");
    if (!keyRes.ok) return "error";
    const { public_key: publicKey } = await keyRes.json();
    if (!publicKey) return "unsupported";

    const registration = await navigator.serviceWorker.register("/sw.js");
    let subscription = await registration.pushManager.getSubscription();
    if (!subscription) {
      subscription = await registration.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: urlBase64ToUint8Array(publicKey),
      });
    }

    const save = await fetch("/api/push/subscribe", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(subscription.toJSON()),
    });
    return save.ok ? "enabled" : "error";
  })();

  try {
    return await pushSetupPromise;
  } finally {
    pushSetupPromise = null;
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
const turnAffordanceSources = new Map();
let turnAffordanceCount = null;

function setTurnAffordanceSource(source, count) {
  const safeCount = Number.isFinite(count) ? Math.max(0, Math.trunc(count)) : 0;
  if (safeCount > 0) {
    turnAffordanceSources.set(source, safeCount);
  } else {
    turnAffordanceSources.delete(source);
  }
  updateTurnAffordances();
}

function updateTurnAffordances() {
  const count = [...turnAffordanceSources.values()]
    .reduce((total, sourceCount) => total + sourceCount, 0);
  if (count === turnAffordanceCount) return;
  turnAffordanceCount = count;
  setFavicon(count > 0);
  setAppBadge(count);
}

function setAppBadge(count) {
  try {
    if (count > 0 && "setAppBadge" in navigator) {
      navigator.setAppBadge(count).catch(() => {});
    } else if (count === 0 && "clearAppBadge" in navigator) {
      navigator.clearAppBadge().catch(() => {});
    }
  } catch {
    /* Badging support is optional and browser-dependent. */
  }
}

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

function shortScoreName(seat) {
  const name = (seat.name || "").trim();
  if (!name) return "open";
  return name;
}

function renderHeaderScores(game, pendingScore) {
  const nav = document.querySelector(".nav");
  if (!nav) return;
  let strip = nav.querySelector(".header-scores");
  if (!strip) {
    strip = document.createElement("div");
    strip.className = "header-scores";
    nav.querySelector(".nav-links").before(strip);
  }
  strip.replaceChildren();
  for (const seat of game.seats) {
    const item = document.createElement("span");
    item.className = "header-score";
    item.title = `${seat.name}: ${seat.score}`;
    if (game.status === "Active" && seat.index === game.turn) {
      item.classList.add("on-turn");
      const dot = document.createElement("span");
      dot.className = "turn-dot";
      item.append(dot);
    }
    const name = document.createElement("span");
    name.className = "header-score-name";
    name.textContent = shortScoreName(seat);
    const value = document.createElement("span");
    value.className = "header-score-value";
    value.textContent = String(seat.score);
    item.append(name, value);
    strip.append(item);
  }
  if (pendingScore != null) {
    const pending = document.createElement("span");
    pending.className = "header-pending-score";
    pending.title = "score for placed tiles";
    pending.textContent = `+${pendingScore}`;
    strip.append(pending);
  }
}

// John Mode helper: valid 2-letter words containing the active rack letter.
function JohnHint({ letter, grandpaMode }) {
  if (!letter) {
    return null;
  }
  const up = letter.toUpperCase();
  const words = TWO_LETTER_WORDS.filter(
    (w) => w.includes(up) && (!grandpaMode || GRANDPA_TWO_LETTER_WORDS.has(w)),
  );
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
  const [hintsRemaining, setHintsRemaining] = useState(
    initial.hints_unlimited ? null : initial.hints_remaining || 0,
  );
  const [hintBusy, setHintBusy] = useState(false);
  const [pushStatus, setPushStatus] = useState(() => notificationSupport());

  const yourTurn = isYourTurn(game);
  const seated = game.your_seat !== null && game.your_seat !== undefined;
  const hasOpenSeat = game.seats.some((s) => s.open);
  const pendingScore = pending.length ? previewScore(game, pending) : null;

  const lastPlaySet = useMemo(() => {
    const s = new Set();
    if (game.last_play) {
      for (const p of game.last_play) s.add(idx(p.row, p.col));
    }
    return s;
  }, [game]);

  useEffect(() => {
    setTurnAffordanceSource("current-game", yourTurn ? 1 : 0);
    return () => setTurnAffordanceSource("current-game", 0);
  }, [yourTurn]);

  useEffect(() => {
    let active = true;
    if (notificationSupport() === "granted") {
      ensurePushNotifications()
        .then((status) => {
          if (active) setPushStatus(status);
        })
        .catch(() => {
          if (active) setPushStatus("error");
        });
    }
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    setHintsRemaining(game.hints_unlimited ? null : game.hints_remaining || 0);
    setHintResult(null);
  }, [game.turn, game.hints_remaining, game.hints_unlimited]);

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

  // Inject live scores into the nav; the server-rendered nav is outside the
  // Preact island.
  useEffect(() => {
    renderHeaderScores(game, pendingScore);
    return () => document.querySelector(".header-scores")?.remove();
  }, [game, pendingScore]);

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

  function reorderRack(fromId, target) {
    setRackOrder((current) => {
      const base =
        current.length === rackTiles.length
          ? current
          : rackTiles.map((_, i) => i);
      return rackOrderAfterInsertion(base, fromId, target) || current;
    });
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

  // Relocate an already-placed (pending) tile to another empty square.
  function movePending(rackId, row, col) {
    if (!yourTurn || mode !== "place") return;
    if (game.board[idx(row, col)].letter) return;
    if (pending.some((p) => p.row === row && p.col === col)) return;
    setPending(
      pending.map((p) => (p.rackId === rackId ? { ...p, row, col } : p)),
    );
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
      setHintsRemaining(data.unlimited ? null : data.remaining);
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
      ensurePushNotifications().catch(() => {});
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

  async function goToNextGame({ activeOnly = false } = {}) {
    try {
      const res = await fetch("/api/my-games");
      if (!res.ok) {
        setError("Could not load your games.");
        return;
      }
      const games = await res.json();
      const others = games.filter((g) => g.id !== gameId);
      const active = others.filter((g) => g.status !== "finished");
      const candidates = activeOnly ? active : (active.length ? active : others);
      if (!candidates.length) {
        setError(activeOnly ? "No active games to go to." : "No other games to go to.");
        return;
      }
      window.location.href = `/games/${candidates[0].id}`;
    } catch {
      setError("Network error — try again.");
    }
  }

  function clearPendingTiles() {
    setPending([]);
    setCursor(null);
    setSelected(null);
  }

  function swapAction() {
    if (mode === "exchange") {
      submitExchange();
      return;
    }
    clearPendingTiles();
    setMode("exchange");
  }

  const finished = game.status === "Finished";
  const hasPendingTiles = pending.length > 0;
  const recallByRackId = (rackId) => recallTile({ rackId });
  const hintsUnlimited = game.hints_unlimited;
  const johnLetter = hoverLetter ||
    (selected != null && rackTiles[selected] && !rackTiles[selected].is_blank
      ? rackTiles[selected].letter
      : null);
  const notificationControl = seated && pushStatus !== "unsupported"
    ? h("div", { class: "notification-controls" },
        pushStatus === "enabled"
          ? html`<span class="muted">Notifications on</span>`
          : pushStatus === "denied"
            ? html`<span class="muted">Notifications blocked</span>`
            : h(
                "button",
                {
                  type: "button",
                  class: "button ghost",
                  disabled: pushStatus === "enabling",
                  onClick: async () => {
                    setPushStatus("enabling");
                    try {
                      setPushStatus(await ensurePushNotifications({ prompt: true }));
                    } catch {
                      setPushStatus("error");
                    }
                  },
                },
                "Enable notifications",
              ))
    : null;
  const boardWrap = h("div", { class: "board-wrap" }, [
    html`<${Board}
      game=${game}
      pending=${pending}
      cursor=${yourTurn && mode === "place" ? cursor : null}
      lastPlaySet=${lastPlaySet}
      onCellClick=${onCellClick}
      onPendingClick=${recallTile}
      onDropTile=${dropTileOnBoard}
      onMovePending=${movePending}
      onRecallPending=${recallByRackId}
    />`,
    !seated && hasOpenSeat ? html`<${JoinForm} gameId=${gameId} />` : null,
  ]);
  let controlButtons = [];
  if (finished) {
    controlButtons = [
      h("button", {
        type: "button",
        class: "button",
        disabled: busy,
        onClick: () => goToNextGame({ activeOnly: true }),
      }, "Next"),
    ];
  } else if (!yourTurn) {
    controlButtons = [
      h("button", {
        type: "button",
        class: "button ghost",
        disabled: busy,
        onClick: shuffleRack,
      }, "Shuffle"),
      h("button", {
        type: "button",
        class: "button",
        disabled: busy,
        onClick: () => goToNextGame(),
      }, "Next"),
    ];
  } else if (hasPendingTiles) {
    controlButtons = [
      h("button", {
        type: "button",
        class: "button ghost",
        disabled: busy,
        onClick: clearPendingTiles,
      }, "Clear"),
      h("button", {
        type: "button",
        class: "button ghost",
        disabled: busy,
        onClick: swapAction,
      }, "Swap"),
      h("button", {
        type: "button",
        class: "button play-button",
        disabled: busy,
        onClick: submitPlay,
      }, `Play ${pendingScore != null ? pendingScore : ""}`.trim()),
    ];
  } else {
    controlButtons = [
      h("button", {
        type: "button",
        class: "button ghost",
        disabled: busy,
        onClick: shuffleRack,
      }, "Shuffle"),
      h("button", {
        type: "button",
        class: "button ghost",
        disabled: busy,
        onClick: swapAction,
      }, "Swap"),
      h("button", {
        type: "button",
        class: "button ghost",
        disabled: busy || mode === "exchange",
        onClick: () => {
          if (window.confirm("Pass your turn?")) postMove({ kind: "pass" });
        },
      }, "Pass"),
    ];
  }
  const controls = h("div", { class: "controls" }, controlButtons);
  const rackArea = seated
    ? h("div", { class: "rack-area" }, [
        !finished
          ? html`<${Rack}
              tiles=${rack}
              selected=${selected}
              mode=${mode}
              exchange=${exchange}
              onSelect=${selectTile}
              onReorder=${reorderRack}
              onPlaceOnBoard=${placeTileOnBoard}
              onRecallPending=${recallByRackId}
              onBackspace=${backspace}
              showBackspace=${mode === "place" && pending.length > 0}
              onHover=${game.john_mode ? setHoverLetter : null}
            />`
          : null,
        error ? html`<p class="move-error">${error}</p>` : null,
        controls,
        !finished && (game.hints_allowed > 0 || hintsUnlimited)
          ? html`<div>
              <button type="button" class="hint-btn"
                disabled=${!yourTurn || hintBusy || (!hintsUnlimited && hintsRemaining <= 0)}
                onClick=${requestHint}>
                ${hintsUnlimited ? "Hint (∞)" : `Hint (${hintsRemaining} left)`}
              </button>
              ${hintResult ? html`<p class="hint-result">${hintResult}</p>` : null}
            </div>`
          : null,
        !finished && game.john_mode
          ? h(
              "div",
              {
                class: `john-tooltip${johnLetter ? " is-visible" : ""}`,
                role: "tooltip",
              },
              html`<${JohnHint}
                letter=${johnLetter}
                grandpaMode=${game.grandpa_mode}
              />`,
            )
          : null,
      ])
    : null;
  const sidebar = html`<aside class="sidebar">
    <div class="game-badges">
      ${game.john_mode ? html`<span class="game-badge">John Mode</span>` : null}
      ${game.grandpa_mode ? html`<span class="game-badge">Grandpa Mode</span>` : null}
      ${game.jax_mode ? html`<span class="game-badge">Jax Mode</span>` : null}
      ${hintsUnlimited ? html`<span class="game-badge">unlimited hints</span>` : null}
      ${game.hints_allowed > 0 ? html`<span class="game-badge">${game.hints_allowed} hint${game.hints_allowed > 1 ? "s" : ""}/player</span>` : null}
    </div>
    <${Scoreboard} game=${game} />
    <p class="muted">Tiles in bag: ${game.bag_count}</p>
    <${MoveLog} game=${game} />
    ${notificationControl}
    <${OtherGames} gameId=${gameId} />
  </aside>`;
  const playColumn = h("div", { class: "play-column" }, [
    boardWrap,
    rackArea,
  ]);
  const layout = h("div", { class: "game-layout" }, [
    playColumn,
    sidebar,
  ]);
  const blankPicker = blankPrompt
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
    : null;

  return h("div", { class: "game" }, [
    html`<h1 class="status">${statusText(game)}</h1>`,
    finished ? html`<${Results} game=${game} />` : null,
    layout,
    blankPicker,
  ]);
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
