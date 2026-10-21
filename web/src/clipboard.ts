// Copy to the clipboard with a fallback for insecure contexts (plain http on a LAN) and old browsers.

/** Copies `input`'s value. Falls back to select + `execCommand("copy")`. Returns whether it worked. */
export async function copyText(input: HTMLInputElement): Promise<boolean> {
  const text = input.value;
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    /* denied or unavailable: use the fallback */
  }
  try {
    input.focus();
    input.select();
    input.setSelectionRange(0, text.length);
    return document.execCommand("copy");
  } catch {
    return false;
  }
}
