const statusEl = document.getElementById("notif-status");
const buttonEl = document.getElementById("notif-enable");

const browserSupport = () => ({
  notifications: "Notification" in window,
  serviceWorker: "serviceWorker" in navigator,
  pushManager: "PushManager" in window,
});

const setStatus = (value) => {
  if (statusEl) statusEl.textContent = value;
};

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
  const registration = await navigator.serviceWorker.getRegistration("/sw.js");
  const subscription = registration
    ? await registration.pushManager.getSubscription()
    : null;
  return { registration, subscription };
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

  const { public_key: publicKey } = await jsonFetch("/api/push/vapid-public-key");
  if (!publicKey) {
    throw new Error("server does not have a VAPID public key");
  }

  const registration = await navigator.serviceWorker.register("/sw.js");
  let subscription = await registration.pushManager.getSubscription();
  if (!subscription) {
    subscription = await registration.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey: urlBase64ToUint8Array(publicKey),
    });
  }
  await jsonFetch("/api/push/subscribe", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(subscription.toJSON()),
  });
}

async function refreshStatus() {
  const support = browserSupport();
  if (!support.notifications || !support.serviceWorker || !support.pushManager) {
    setStatus("Notifications aren't supported in this browser.");
    if (buttonEl) buttonEl.hidden = true;
    return;
  }

  const permission = Notification.permission;
  if (permission === "denied") {
    setStatus("Notifications are blocked in your browser settings.");
    if (buttonEl) buttonEl.hidden = true;
    return;
  }

  try {
    const { subscription } = await registrationAndSubscription();
    if (permission === "granted" && subscription) {
      setStatus("Notifications are on.");
      if (buttonEl) buttonEl.hidden = true;
      return;
    }
    if (permission === "granted" && !subscription) {
      if (buttonEl) buttonEl.hidden = true;
      await enableNotifications();
      setStatus("Notifications are on.");
      return;
    }
  } catch {
    setStatus("Couldn't enable notifications.");
    if (buttonEl) buttonEl.hidden = true;
    return;
  }

  setStatus("Notifications are off.");
  if (buttonEl) {
    buttonEl.hidden = false;
    buttonEl.textContent = "Enable notifications";
  }
}

if (buttonEl) {
  buttonEl.addEventListener("click", async () => {
    buttonEl.disabled = true;
    setStatus("Enabling notifications…");
    try {
      await enableNotifications();
      await refreshStatus();
    } catch {
      setStatus("Couldn't enable notifications.");
    } finally {
      buttonEl.disabled = false;
    }
  });
}

refreshStatus().catch(() => {
  setStatus("Couldn't enable notifications.");
  if (buttonEl) buttonEl.hidden = true;
});
