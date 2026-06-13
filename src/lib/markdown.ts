import { Marked } from "marked";
import { markedHighlight } from "marked-highlight";
import hljs from "highlight.js";
import DOMPurify from "dompurify";
import "highlight.js/styles/github-dark.css";

const marked = new Marked(
  { gfm: true, breaks: true },
  markedHighlight({
    langPrefix: "hljs language-",
    highlight(code, lang) {
      const language = hljs.getLanguage(lang) ? lang : "plaintext";
      return hljs.highlight(code, { language }).value;
    },
  }),
);

const ALLOWED_TAGS = [
  "p", "br", "strong", "em", "del", "a", "ul", "ol", "li",
  "h1", "h2", "h3", "h4", "h5", "h6", "blockquote", "code", "pre",
  "hr", "table", "thead", "tbody", "tr", "th", "td", "span",
];
const ALLOWED_ATTR = ["href", "title", "class"];

const SAFE_SCHEMES = /^(https?:|mailto:)/i;

DOMPurify.addHook("afterSanitizeAttributes", (node) => {
  if (node.tagName === "A") {
    const href = node.getAttribute("href");
    if (href && SAFE_SCHEMES.test(href)) {
      node.setAttribute("target", "_blank");
      node.setAttribute("rel", "noopener noreferrer nofollow");
    } else if (href) {
      node.removeAttribute("href");
    }
  }
});

export function renderMarkdown(source: string): string {
  const raw = marked.parse(source ?? "") as string;
  return DOMPurify.sanitize(raw, { ALLOWED_TAGS, ALLOWED_ATTR });
}
