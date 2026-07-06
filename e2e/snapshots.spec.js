import { test, expect } from "@playwright/test";
import fs from "node:fs";

// Signs in as a fixture user via the dev-mode (PASSKEY_DISABLED) login flow,
// which grants a session cookie on username alone.
async function signIn(page, username) {
  const res = await page.request.post("/auth/login/begin", {
    data: { username },
  });
  if (!res.ok()) throw new Error(`login ${username}: ${res.status()}`);
}

// The active fixture game (Scott vs a medium bot with Scott/Shelli modes on).
function activeGameUrl() {
  const dir = "e2e/fixture-data/games";
  for (const file of fs.readdirSync(dir)) {
    const game = JSON.parse(fs.readFileSync(`${dir}/${file}`, "utf8"));
    if (game.scott_mode) return `/games/${game.id}`;
  }
  throw new Error("fixture is missing the Scott Mode game");
}

test("home page, signed out", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveScreenshot("home-signed-out.png", { fullPage: true });
});

test("home page, signed in", async ({ page }) => {
  await signIn(page, "scott");
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Open games" })).toBeVisible();
  await expect(page).toHaveScreenshot("home-signed-in.png", { fullPage: true });
});

test("new game page", async ({ page }) => {
  await signIn(page, "scott");
  await page.goto("/games/new");
  await expect(page.getByRole("heading", { name: "New game" })).toBeVisible();
  await expect(page).toHaveScreenshot("new-game.png", { fullPage: true });
});

test("game page", async ({ page }) => {
  await signIn(page, "scott");
  await page.goto(activeGameUrl());
  // Wait for the Preact island to hydrate (the rack renders client-side).
  await expect(page.locator(".rack .rack-tile").first()).toBeVisible();
  await expect(page).toHaveScreenshot("game.png", { fullPage: true });
});
