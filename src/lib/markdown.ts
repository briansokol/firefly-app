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

export function renderMarkdown(source: string): string {
  const raw = marked.parse(source ?? "") as string;
  return DOMPurify.sanitize(raw, { ALLOWED_TAGS, ALLOWED_ATTR });
}
