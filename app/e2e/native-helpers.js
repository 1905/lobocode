import fs from "node:fs";
import path from "node:path";

// Log only button identity. Inputs, config values and credentials never enter it.
export async function click(text, selector = `button*=${text}`) {
  const button = await $(selector);
  const identity = await button.getAttribute("aria-label");
  const tag = await button.getTagName();
  const type = await button.getAttribute("type");
  fs.appendFileSync(path.join(process.env.LOBO_E2E_ROOT, "click-trace.jsonl"),
    JSON.stringify({ at: Date.now(), requested: text, tag, type, ariaLabel: identity }) + "\n");
  await button.click();
}

export async function fits() {
  await browser.waitUntil(async () => browser.execute(() => {
    const doc = document.documentElement;
    const surface = document.querySelector(".surface")?.getBoundingClientRect();
    const visible = [...document.querySelectorAll("button,input,[role='tab']")]
      .filter((element) => element.getClientRects().length > 0);
    return !!surface && doc.scrollWidth <= doc.clientWidth && doc.scrollHeight <= doc.clientHeight
      && surface.bottom <= innerHeight + 1 && surface.right <= innerWidth + 1
      && visible.every((element) => {
        const r = element.getBoundingClientRect();
        return r.top >= 0 && r.left >= 0 && r.bottom <= innerHeight + 1 && r.right <= innerWidth + 1;
      });
  }), { timeoutMsg: "native controls must fit without scrolling or clipping" });
}
