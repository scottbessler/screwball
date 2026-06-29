const SIZE = 15;
const CENTER = 7;
const STORAGE_KEY = "screwball.touchDebug.offsets.v1";

const POINTS = {
  A: 1, B: 3, C: 3, D: 2, E: 1, F: 4, G: 2, H: 4, I: 1, J: 8,
  K: 5, L: 1, M: 3, N: 1, O: 1, P: 3, Q: 10, R: 1, S: 1, T: 1,
  U: 1, V: 4, W: 4, X: 8, Y: 4, Z: 10,
};

const PREMIUM_LABEL = { dl: "DL", tl: "TL", dw: "DW", tw: "TW", none: "" };
const PREMIUMS = {
  tw: [[0, 0], [0, 7], [0, 14], [7, 0], [7, 14], [14, 0], [14, 7], [14, 14]],
  dw: [
    [1, 1], [2, 2], [3, 3], [4, 4], [1, 13], [2, 12], [3, 11], [4, 10],
    [10, 4], [11, 3], [12, 2], [13, 1], [10, 10], [11, 11], [12, 12], [13, 13],
  ],
  tl: [[1, 5], [1, 9], [5, 1], [5, 5], [5, 9], [5, 13], [9, 1], [9, 5], [9, 9], [9, 13], [13, 5], [13, 9]],
  dl: [
    [0, 3], [0, 11], [2, 6], [2, 8], [3, 0], [3, 7], [3, 14], [6, 2],
    [6, 6], [6, 8], [6, 12], [7, 3], [7, 11], [8, 2], [8, 6], [8, 8],
    [8, 12], [11, 0], [11, 7], [11, 14], [12, 6], [12, 8], [14, 3], [14, 11],
  ],
};

const boardEl = document.getElementById("touch-debug-board");
const rackEl = document.getElementById("touch-debug-rack");
const controlsEl = document.getElementById("touch-debug-controls");

const state = {
  board: [],
  rack: [],
  offsets: loadOffsets(),
  drag: null,
  markers: {
    touch: marker("touch-debug-marker touch-debug-touch-marker"),
    drag: marker("touch-debug-marker touch-debug-drag-marker"),
    drop: marker("touch-debug-marker touch-debug-drop-marker"),
  },
};

function idx(row, col) {
  return row * SIZE + col;
}

function tileLiftPx() {
  const fontSize = Number.parseFloat(
    getComputedStyle(document.documentElement).fontSize,
  );
  return 2.4 * (Number.isFinite(fontSize) ? fontSize : 16);
}

function proposedOffsets() {
  const lift = tileLiftPx();
  return {
    dropX: 0,
    dropY: Math.round(-lift * 0.4),
    tileX: 0,
    tileY: Math.round(-lift),
  };
}

function oldTargetOffsets() {
  const lift = tileLiftPx();
  return {
    dropX: 0,
    dropY: Math.round(-lift),
    tileX: 0,
    tileY: Math.round(-lift),
  };
}

function loadOffsets() {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY));
    if (saved && ["dropX", "dropY", "tileX", "tileY"].every((key) => Number.isFinite(saved[key]))) {
      return saved;
    }
  } catch {
    /* Ignore bad local debug state. */
  }
  return proposedOffsets();
}

function saveOffsets() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state.offsets));
}

function marker(className) {
  const el = document.createElement("div");
  el.className = className;
  el.hidden = true;
  document.body.append(el);
  return el;
}

function premiumFor(row, col) {
  for (const [code, cells] of Object.entries(PREMIUMS)) {
    if (cells.some(([r, c]) => r === row && c === col)) return code;
  }
  return row === CENTER && col === CENTER ? "dw" : "none";
}

function resetTiles() {
  state.board = Array.from({ length: SIZE * SIZE }, (_, index) => {
    const row = Math.floor(index / SIZE);
    const col = index % SIZE;
    return { row, col, premium: premiumFor(row, col), tile: null };
  });
  placeWord(7, 5, 0, 1, "TOUCH");
  placeWord(4, 11, 1, 0, "DEBUG");
  state.rack = "PLANETS".split("").map((letter, id) => ({
    id,
    letter,
    used: false,
  }));
  renderAll();
}

function placeWord(row, col, dr, dc, word) {
  for (let i = 0; i < word.length; i++) {
    state.board[idx(row + dr * i, col + dc * i)].tile = {
      letter: word[i],
      rackId: null,
      pending: false,
    };
  }
}

function tilePoints(letter) {
  return POINTS[letter] || 0;
}

function renderAll() {
  renderBoard();
  renderRack();
  syncControls();
}

function renderBoard() {
  boardEl.replaceChildren();
  for (const square of state.board) {
    const cell = document.createElement("div");
    cell.className = cellClass(square);
    cell.dataset.row = String(square.row);
    cell.dataset.col = String(square.col);
    cell.setAttribute("role", "gridcell");
    if (square.tile) {
      cell.append(tileFace(square.tile));
      if (square.tile.pending) {
        cell.classList.add("pending-cell");
        cell.dataset.rackid = String(square.tile.rackId);
        cell.addEventListener("pointerdown", (event) => startDrag(event, {
          source: "board",
          rackId: square.tile.rackId,
          letter: square.tile.letter,
        }));
      }
    } else {
      const label = document.createElement("span");
      label.className = "premium-label";
      label.textContent = square.row === CENTER && square.col === CENTER
        ? "*"
        : PREMIUM_LABEL[square.premium];
      cell.append(label);
    }
    boardEl.append(cell);
  }
}

function cellClass(square) {
  if (square.tile && square.tile.pending) return "cell tile pending-cell";
  if (square.tile) return "cell tile";
  return ["cell", square.premium === "none" ? "" : `premium-${square.premium}`]
    .filter(Boolean)
    .join(" ");
}

function tileFace(tile) {
  const face = document.createElement("span");
  face.className = tile.pending ? "tile-face pending" : "tile-face";
  const letter = document.createElement("span");
  letter.className = "tile-letter";
  letter.textContent = tile.letter;
  const points = document.createElement("span");
  points.className = "tile-points";
  points.textContent = String(tilePoints(tile.letter));
  face.append(letter, points);
  return face;
}

function renderRack() {
  rackEl.replaceChildren();
  for (const tile of state.rack) {
    if (tile.used) {
      const slot = document.createElement("div");
      slot.className = "rack-tile rack-slot";
      rackEl.append(slot);
      continue;
    }
    const button = document.createElement("button");
    button.type = "button";
    button.className = "rack-tile";
    button.dataset.tileId = String(tile.id);
    button.setAttribute("aria-label", `Drag ${tile.letter}`);
    button.append(tilePart("tile-letter", tile.letter), tilePart("tile-points", tilePoints(tile.letter)));
    button.addEventListener("pointerdown", (event) => startDrag(event, {
      source: "rack",
      rackId: tile.id,
      letter: tile.letter,
    }));
    rackEl.append(button);
  }
}

function tilePart(className, value) {
  const span = document.createElement("span");
  span.className = className;
  span.textContent = String(value);
  return span;
}

function startDrag(event, drag) {
  if (event.button != null && event.button !== 0) return;
  event.preventDefault();
  const sourceEl = event.currentTarget;
  sourceEl.setPointerCapture?.(event.pointerId);
  state.drag = {
    ...drag,
    pointerId: event.pointerId,
    startX: event.clientX,
    startY: event.clientY,
    x: event.clientX,
    y: event.clientY,
    sourceEl,
    ghost: null,
    currentCell: null,
  };
  window.addEventListener("pointermove", moveDrag, { passive: false });
  window.addEventListener("pointerup", endDrag, { passive: false });
  window.addEventListener("pointercancel", cancelDrag, { passive: false });
}

function moveDrag(event) {
  const drag = state.drag;
  if (!drag || event.pointerId !== drag.pointerId) return;
  event.preventDefault();
  drag.x = event.clientX;
  drag.y = event.clientY;
  if (!drag.ghost && Math.abs(drag.x - drag.startX) + Math.abs(drag.y - drag.startY) > 6) {
    drag.sourceEl.classList.add("dragging");
    drag.ghost = document.createElement("div");
    drag.ghost.className = "rack-tile-ghost touch-debug-ghost";
    drag.ghost.textContent = drag.letter;
    document.body.append(drag.ghost);
  }
  if (!drag.ghost) return;
  updateDragVisuals(drag);
}

function updateDragVisuals(drag) {
  const dragged = dragPoint(drag.x, drag.y);
  const drop = dropPoint(drag.x, drag.y);
  drag.ghost.style.setProperty("--drag-x", `${dragged.x}px`);
  drag.ghost.style.setProperty("--drag-y", `${dragged.y}px`);

  positionMarker(state.markers.touch, drag.x, drag.y);
  positionMarker(state.markers.drag, dragged.x, dragged.y);
  positionMarker(state.markers.drop, drop.x, drop.y);

  clearBoardDropGhost();
  const cell = emptyCellAtPoint(drop.x, drop.y);
  drag.currentCell = cell;
  if (cell) {
    showBoardDropGhost(cell, drag.letter);
  }
  updateReadout(drag, dragged, drop, cell);
}

function endDrag(event) {
  const drag = state.drag;
  if (!drag || event.pointerId !== drag.pointerId) return;
  if (drag.ghost) {
    const drop = dropPoint(event.clientX, event.clientY);
    const cell = drag.currentCell || emptyCellAtPoint(drop.x, drop.y);
    if (drag.source === "board" && pointOverRack(event.clientX, event.clientY)) {
      recallTile(drag.rackId);
    } else if (cell) {
      commitDrop(drag, Number(cell.dataset.row), Number(cell.dataset.col));
    }
  }
  cleanupDrag();
}

function cancelDrag(event) {
  if (!state.drag || event.pointerId !== state.drag.pointerId) return;
  cleanupDrag();
}

function cleanupDrag() {
  if (state.drag) {
    state.drag.sourceEl.classList.remove("dragging");
    state.drag.ghost?.remove();
  }
  state.drag = null;
  clearBoardDropGhost();
  hideMarkers();
  window.removeEventListener("pointermove", moveDrag);
  window.removeEventListener("pointerup", endDrag);
  window.removeEventListener("pointercancel", cancelDrag);
}

function commitDrop(drag, row, col) {
  if (drag.source === "board") {
    removePending(drag.rackId);
  }
  const square = state.board[idx(row, col)];
  if (square.tile) return;
  square.tile = { letter: drag.letter, rackId: drag.rackId, pending: true };
  const rackTile = state.rack.find((tile) => tile.id === drag.rackId);
  if (rackTile) rackTile.used = true;
  renderAll();
}

function recallTile(rackId) {
  removePending(rackId);
  const rackTile = state.rack.find((tile) => tile.id === rackId);
  if (rackTile) rackTile.used = false;
  renderAll();
}

function removePending(rackId) {
  for (const square of state.board) {
    if (square.tile && square.tile.pending && square.tile.rackId === rackId) {
      square.tile = null;
    }
  }
}

function dropPoint(x, y) {
  return {
    x: x + state.offsets.dropX,
    y: y + state.offsets.dropY,
  };
}

function dragPoint(x, y) {
  return {
    x: x + state.offsets.tileX,
    y: y + state.offsets.tileY,
  };
}

function emptyCellAtPoint(x, y) {
  const offsets = [[0, 0], [0, -16], [0, 16], [-16, 0], [16, 0]];
  for (const [ox, oy] of offsets) {
    const el = document.elementFromPoint(x + ox, y + oy);
    const cell = el && el.closest(".cell");
    if (cell && boardEl.contains(cell) && !cell.classList.contains("tile")) {
      return cell;
    }
  }
  return null;
}

function pointOverRack(x, y) {
  const rect = rackEl.getBoundingClientRect();
  return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
}

function showBoardDropGhost(cell, letter) {
  cell.classList.add("drag-over", "board-drop-ghost");
  cell.dataset.dropLetter = letter;
  cell.dataset.dropPoints = String(tilePoints(letter));
  cell.dataset.dropBlank = "false";
}

function clearBoardDropGhost() {
  for (const cell of boardEl.querySelectorAll(".board-drop-ghost,.drag-over")) {
    cell.classList.remove("board-drop-ghost", "drag-over");
    delete cell.dataset.dropLetter;
    delete cell.dataset.dropPoints;
    delete cell.dataset.dropBlank;
  }
}

function positionMarker(el, x, y) {
  el.hidden = false;
  el.style.setProperty("--marker-x", `${x}px`);
  el.style.setProperty("--marker-y", `${y}px`);
}

function hideMarkers() {
  for (const el of Object.values(state.markers)) {
    el.hidden = true;
  }
  updateReadout(null);
}

function updateReadout(drag, dragged, drop, cell) {
  setText("touch-debug-touch-readout", drag ? pointLabel({ x: drag.x, y: drag.y }) : "-");
  setText("touch-debug-drag-readout", dragged ? pointLabel(dragged) : "-");
  setText("touch-debug-drop-readout", drop ? pointLabel(drop) : "-");
  setText(
    "touch-debug-cell-readout",
    cell ? `${Number(cell.dataset.row) + 1}, ${Number(cell.dataset.col) + 1}` : "-",
  );
}

function pointLabel(point) {
  return `${Math.round(point.x)}, ${Math.round(point.y)}`;
}

function setText(id, value) {
  const el = document.getElementById(id);
  if (el) el.textContent = value;
}

function syncControls() {
  for (const key of ["dropX", "dropY", "tileX", "tileY"]) {
    const wrap = controlsEl.querySelector(`[data-offset-key="${key}"]`);
    const input = wrap?.querySelector("input");
    const output = wrap?.querySelector("output");
    if (input) input.value = String(state.offsets[key]);
    if (output) output.textContent = `${state.offsets[key]} px`;
  }
}

function setOffsets(next) {
  state.offsets = { ...state.offsets, ...next };
  saveOffsets();
  syncControls();
  if (state.drag?.ghost) updateDragVisuals(state.drag);
}

controlsEl.addEventListener("input", (event) => {
  const input = event.target.closest("input[type='range']");
  if (!input) return;
  const wrap = input.closest("[data-offset-key]");
  setOffsets({ [wrap.dataset.offsetKey]: Number(input.value) });
});

controlsEl.addEventListener("click", (event) => {
  const nudge = event.target.closest("[data-nudge]");
  if (!nudge) return;
  const wrap = nudge.closest("[data-offset-key]");
  const key = wrap.dataset.offsetKey;
  const value = Math.max(-140, Math.min(140, state.offsets[key] + Number(nudge.dataset.nudge)));
  setOffsets({ [key]: value });
});

document.getElementById("touch-debug-preset-proposed")?.addEventListener("click", () => {
  setOffsets(proposedOffsets());
});

document.getElementById("touch-debug-preset-old")?.addEventListener("click", () => {
  setOffsets(oldTargetOffsets());
});

document.getElementById("touch-debug-reset-board")?.addEventListener("click", resetTiles);

resetTiles();
