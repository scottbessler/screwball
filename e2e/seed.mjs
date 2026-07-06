// Seeds a fresh DATA_PATH with the deterministic fixture used by the
// Playwright snapshot tests. Run against a locally started server:
//
//   DATA_PATH=e2e/fixture-data PASSKEY_DISABLED=1 PORT=8123 ./target/debug/screwball &
//   node e2e/seed.mjs
//
// The resulting e2e/fixture-data directory is committed; regenerate it only
// when the game state format changes (screenshots will need re-approval).

const BASE = process.env.BASE_URL || "http://localhost:8123";

function cookieFrom(res) {
  const raw = res.headers.get("set-cookie");
  if (!raw) throw new Error("expected a session cookie");
  return raw.split(";")[0];
}

async function register(username, displayName) {
  const res = await fetch(`${BASE}/auth/register/begin`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ username, display_name: displayName }),
  });
  if (!res.ok) throw new Error(`register ${username}: ${res.status}`);
  return cookieFrom(res);
}

async function createGame(cookie, form) {
  const res = await fetch(`${BASE}/games`, {
    method: "POST",
    headers: {
      "content-type": "application/x-www-form-urlencoded",
      cookie,
    },
    body: form,
    redirect: "manual",
  });
  if (res.status !== 303) throw new Error(`create game: ${res.status}`);
  return res.headers.get("location");
}

async function gameState(cookie, location) {
  const res = await fetch(`${BASE}${location}/state`, { headers: { cookie } });
  if (!res.ok) throw new Error(`state: ${res.status}`);
  return res.json();
}

// Brute-force a legal opening play from the rack: try 2-letter and 3-letter
// horizontal placements through the center until the server accepts one.
async function playSomething(cookie, location) {
  const state = await gameState(cookie, location);
  const letters = state.your_rack
    .filter((t) => !t.is_blank)
    .map((t) => t.letter);
  const attempts = [];
  for (let i = 0; i < letters.length; i++) {
    for (let j = 0; j < letters.length; j++) {
      if (i === j) continue;
      attempts.push([letters[i], letters[j]]);
      for (let k = 0; k < letters.length; k++) {
        if (k === i || k === j) continue;
        attempts.push([letters[i], letters[j], letters[k]]);
      }
    }
  }
  // Attempts must run sequentially: each is a real move submission and the
  // first accepted one ends the turn.
  for (const word of attempts) {
    const placements = word.map((letter, n) => ({
      row: 7,
      col: 7 + n,
      letter,
      is_blank: false,
    }));
    // eslint-disable-next-line no-await-in-loop
    const res = await fetch(`${BASE}${location}/move`, {
      method: "POST",
      headers: { "content-type": "application/json", cookie },
      body: JSON.stringify({ kind: "play", placements }),
    });
    if (res.ok) return;
  }
  throw new Error(`no playable word found in rack ${letters.join("")}`);
}

const scott = await register("scott", "Scott");
const shelli = await register("shelli", "Shelli");

// An active game showcasing the new modes: Scott Mode annotates the move log,
// Shelli Mode restricts the bot, and hints are limited.
const active = await createGame(
  scott,
  "seat2=medium&scott_mode=on&shelli_mode=on&hints=2",
);
await playSomething(scott, active);

// An open two-human game hosted by Shelli, joinable by Scott.
await createGame(shelli, "seat2=open&grandpa_mode=on");

// A game that the fixture marks as finished (status edited on disk afterwards
// by regenerate.sh) so the home page shows a Finished section.
const finished = await createGame(scott, "seat2=easy&jax_mode=on");

console.log(JSON.stringify({ active, finished }));
