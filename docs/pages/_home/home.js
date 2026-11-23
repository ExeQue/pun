// Pun homepage behavior: the install copy button, the command switcher and the pun check run.
// The page makes no speed figures or comparisons; it says Pun is fast in words only.

const reduced = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

function onView(el, fn, threshold = 0.35) {
  if (!el) return;
  if (!("IntersectionObserver" in window)) return fn();
  const io = new IntersectionObserver((entries) => {
    for (const e of entries) if (e.isIntersecting) { io.disconnect(); fn(); }
  }, { threshold });
  io.observe(el);
}

function installCopy(root) {
  for (const box of root.querySelectorAll("[data-install]")) {
    const text = box.querySelector("[data-command-text]");
    const copy = box.querySelector("[data-copy]");
    copy?.addEventListener("click", async () => {
      try { await navigator.clipboard.writeText(text.textContent); } catch {}
      copy.textContent = "copied";
      setTimeout(() => (copy.textContent = "copy"), 1200);
    });
  }
}

// The output is visible by default. When the terminal scrolls into view, or on "Run again",
// the lines replay in quick succession: the run is over almost as soon as it starts.
function run(root) {
  const sec = root.querySelector("[data-run]");
  if (!sec) return;
  const play = () => {
    if (reduced()) return;
    sec.classList.remove("go");
    void sec.offsetWidth;
    sec.classList.add("go");
  };
  sec.querySelector("[data-run-replay]")?.addEventListener("click", play);
  onView(sec.querySelector(".term"), play, 0.5);
}

// The command switcher is a tab list. While it is on screen it steps to the next command each time the active tab's
// bar fills; hovering or focusing it pauses the bar, and the first click or key press stops the stepping for good.
// With reduced motion it never steps on its own.
function commands(root) {
  const box = root.querySelector("[data-cmds]");
  if (!box) return;
  const tabs = [...box.querySelectorAll('[role="tab"]')];
  const panels = tabs.map((t) => box.querySelector("#" + t.getAttribute("aria-controls")));
  const matches = [...box.querySelectorAll("[data-match]")];
  let at = 0;
  const show = (n, focus) => {
    at = (n + tabs.length) % tabs.length;
    tabs.forEach((t, k) => {
      const on = k === at;
      t.setAttribute("aria-selected", String(on));
      t.tabIndex = on ? 0 : -1;
      panels[k].classList.toggle("on", on);
      panels[k].classList.remove("go");
      if (on) panels[k].removeAttribute("aria-hidden"); else panels[k].setAttribute("aria-hidden", "true");
      matches[k]?.classList.toggle("on", on);
      if (on) matches[k]?.removeAttribute("aria-hidden"); else matches[k]?.setAttribute("aria-hidden", "true");
    });
    if (focus) tabs[at].focus();
    if (!reduced()) { void panels[at].offsetWidth; panels[at].classList.add("go"); }
  };
  const stop = () => box.classList.remove("auto");
  tabs.forEach((t, k) => {
    t.addEventListener("click", () => { stop(); show(k); });
    t.querySelector("i")?.addEventListener("animationend", () => show(at + 1));
  });
  box.addEventListener("keydown", (e) => {
    const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[e.key];
    const jump = { Home: 0, End: tabs.length - 1 }[e.key];
    if (step === undefined && jump === undefined) return;
    if (!tabs.includes(document.activeElement)) return;
    e.preventDefault();
    stop();
    show(jump ?? at + step, true);
  });
  if (reduced() || !("IntersectionObserver" in window)) return;
  const io = new IntersectionObserver(([e]) => box.classList.toggle("away", !e.isIntersecting), { threshold: 0.4 });
  io.observe(box);
  box.classList.add("auto");
}

export function initHome(root = document) {
  const home = root.querySelector(".pun-home");
  if (!home || home.dataset.ready) return;
  home.dataset.ready = "1";
  installCopy(home);
  commands(home);
  run(home);
}

initHome();
