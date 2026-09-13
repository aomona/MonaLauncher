import DOMPurify from "dompurify";

// RSS/cache content is untrusted even when it comes from our own publishing workflow.
export function sanitizeArticle(content: string): string {
  const fragment = DOMPurify.sanitize(content, {
    ALLOWED_TAGS: [
      "p",
      "br",
      "h1",
      "h2",
      "h3",
      "h4",
      "h5",
      "h6",
      "strong",
      "em",
      "del",
      "ul",
      "ol",
      "li",
      "blockquote",
      "pre",
      "code",
      "hr",
      "a",
      "img",
      "table",
      "thead",
      "tbody",
      "tr",
      "th",
      "td",
    ],
    ALLOWED_ATTR: ["href", "title", "start", "src", "alt"],
    ALLOW_DATA_ATTR: false,
    ALLOW_ARIA_ATTR: false,
    ALLOWED_URI_REGEXP: /^https:\/\//i,
    RETURN_DOM_FRAGMENT: true,
  });
  for (const anchor of fragment.querySelectorAll("a[href]")) {
    try {
      const url = new URL(anchor.getAttribute("href")!);
      if (url.protocol !== "https:" || url.username || url.password) anchor.removeAttribute("href");
    } catch {
      anchor.removeAttribute("href");
    }
  }
  for (const image of fragment.querySelectorAll("img")) {
    try {
      const url = new URL(image.getAttribute("src") ?? "");
      if (url.protocol !== "https:" || url.username || url.password)
        throw new Error("Unsupported image URL");
      image.src = url.href;
      image.alt = image.getAttribute("alt") ?? "";
      image.loading = "lazy";
      image.decoding = "async";
      image.referrerPolicy = "no-referrer";
    } catch {
      image.replaceWith(document.createTextNode(image.getAttribute("alt") ?? ""));
    }
  }
  const container = document.createElement("div");
  container.append(fragment);
  return container.innerHTML;
}
