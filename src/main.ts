import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

// Suppress the WebView's default right-click menu (Reload / Save As / Share /
// Inspect) — useless and unpolished for a desktop app. Applied in every build,
// including dev (DevTools is still reachable via F12 / Ctrl+Shift+I). The only
// carve-out is editable fields, where native Cut/Copy/Paste is genuinely useful
// for composing.
document.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement | null;
  const editable =
    !!t &&
    (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable);
  if (!editable) e.preventDefault();
});

const target = document.getElementById("app");
if (!target) throw new Error("missing #app");

mount(App, { target });
