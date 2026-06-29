const state = {
  publicKey: null,
};

const status = (id, value) => {
  const el = document.getElementById(id);
  if (el) el.textContent = value;
};

const output = (value) => {
  const el = document.getElementById("debug-output");
  if (!el) return;
  el.textContent = typeof value === "string" ? value : JSON.stringify(value, null, 2);
};

function browserSupport() {
  return {
    notifications: "Notification" in window,
    serviceWorker: "serviceWorker" in navigator,
    pushManager: "PushManager" in window,
  };
}

function supportLabel() {
  const support = browserSupport();
  const missing = Object.entries(support)
    .filter(([, ok]) => !ok)
    .map(([name]) => name);
  return missing.length ? `missing ${missing.join(", ")}` : "supported";
}

function urlBase64ToUint8Array(value) {
  const padding = "=".repeat((4 - (value.length % 4)) % 4);
  const base64 = (value + padding).replace(/-/g, "+").replace(/_/g, "/");
  const raw = window.atob(base64);
  const bytes = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) {
    bytes[i] = raw.charCodeAt(i);
  }
  return bytes;
}

async function jsonFetch(url, options = {}) {
  const response = await fetch(url, options);
  const text = await response.text();
  let body = null;
  try {
    body = text ? JSON.parse(text) : null;
  } catch {
    body = text;
  }
  if (!response.ok) {
    const message = body && body.error ? body.error : `HTTP ${response.status}`;
    throw new Error(message);
  }
  return body;
}

async function registrationAndSubscription() {
  if (!("serviceWorker" in navigator) || !("PushManager" in window)) {
    return { registration: null, subscription: null };
  }
  const registration = await navigator.serviceWorker.getRegistration("/sw.js");
  const subscription = registration
    ? await registration.pushManager.getSubscription()
    : null;
  return { registration, subscription };
}

async function refreshStatus() {
  status("debug-support", supportLabel());
  status(
    "debug-permission",
    "Notification" in window ? Notification.permission : "unsupported",
  );

  try {
    const { registration, subscription } = await registrationAndSubscription();
    status("debug-service-worker", registration ? "registered" : "not registered");
    status("debug-subscription", subscription ? "present" : "none");
  } catch (err) {
    status("debug-service-worker", `error: ${err.message}`);
    status("debug-subscription", "unknown");
  }

  try {
    const server = await jsonFetch("/api/push/debug");
    state.publicKey = server.public_key;
    status("debug-server-configured", server.configured ? "configured" : "not configured");
    status("debug-server-subscriptions", String(server.stored_subscriptions));
    status("debug-server-key", server.public_key ? "present" : "missing");
  } catch (err) {
    status("debug-server-configured", `error: ${err.message}`);
    status("debug-server-subscriptions", "unknown");
    status("debug-server-key", "unknown");
  }
}

async function enableNotifications() {
  const support = browserSupport();
  if (!support.notifications || !support.serviceWorker || !support.pushManager) {
    throw new Error("this browser does not support all notification APIs");
  }
  let permission = Notification.permission;
  if (permission === "default") {
    permission = await Notification.requestPermission();
  }
  if (permission !== "granted") {
    throw new Error(`notification permission is ${permission}`);
  }

  const key = state.publicKey ||
    (await jsonFetch("/api/push/vapid-public-key")).public_key;
  if (!key) {
    throw new Error("server does not have a VAPID public key");
  }

  const registration = await navigator.serviceWorker.register("/sw.js");
  let subscription = await registration.pushManager.getSubscription();
  if (!subscription) {
    subscription = await registration.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey: urlBase64ToUint8Array(key),
    });
  }
  await jsonFetch("/api/push/subscribe", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(subscription.toJSON()),
  });
  return {
    ok: true,
    endpoint: subscription.endpoint,
  };
}

async function localNotification() {
  if (!("Notification" in window) || Notification.permission !== "granted") {
    throw new Error("notifications are not granted");
  }
  if (!("serviceWorker" in navigator)) {
    throw new Error("service workers are not supported");
  }
  const registration = await navigator.serviceWorker.register("/sw.js");
  await registration.showNotification("Screwball local notification test", {
    body: "If you can see this, browser notification display is working.",
    icon: "/public/apple-touch-icon.png",
    badge: "/public/apple-touch-icon.png",
    tag: "screwball-local-notification-debug",
    data: { url: "/debug/notifications" },
  });
  return { ok: true };
}

async function serverNotification() {
  return await jsonFetch("/api/push/test", { method: "POST" });
}

async function unsubscribe() {
  const { subscription } = await registrationAndSubscription();
  if (!subscription) return { ok: true, message: "no browser subscription" };
  await jsonFetch("/api/push/unsubscribe", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ endpoint: subscription.endpoint }),
  });
  const removed = await subscription.unsubscribe();
  return { ok: true, browser_unsubscribed: removed };
}

function bind(id, fn) {
  const button = document.getElementById(id);
  if (!button) return;
  button.addEventListener("click", async () => {
    button.disabled = true;
    output("working...");
    try {
      const result = await fn();
      output(result);
    } catch (err) {
      output(`Error: ${err.message}`);
    } finally {
      button.disabled = false;
      await refreshStatus();
    }
  });
}

bind("debug-enable", enableNotifications);
bind("debug-local-test", localNotification);
bind("debug-server-test", serverNotification);
bind("debug-unsubscribe", unsubscribe);
refreshStatus().catch((err) => output(`Error: ${err.message}`));
