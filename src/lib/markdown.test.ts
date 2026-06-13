import { describe, it, expect } from "vitest";
import { renderMarkdown } from "./markdown";

describe("renderMarkdown — GFM", () => {
  it("renders bold", () => {
    expect(renderMarkdown("**hi**")).toContain("<strong>hi</strong>");
  });
  it("renders italic", () => {
    expect(renderMarkdown("*hi*")).toContain("<em>hi</em>");
  });
  it("renders headings", () => {
    expect(renderMarkdown("# Title")).toContain("<h1>Title</h1>");
  });
  it("renders unordered lists", () => {
    const html = renderMarkdown("- a\n- b");
    expect(html).toContain("<ul>");
    expect(html).toContain("<li>a</li>");
  });
  it("renders inline code", () => {
    expect(renderMarkdown("`x`")).toContain("<code>x</code>");
  });
  it("renders blockquotes", () => {
    expect(renderMarkdown("> quote")).toContain("<blockquote>");
  });
  it("renders GFM tables", () => {
    const html = renderMarkdown("| a | b |\n| - | - |\n| 1 | 2 |");
    expect(html).toContain("<table>");
    expect(html).toContain("<th>a</th>");
  });
  it("renders GFM strikethrough", () => {
    expect(renderMarkdown("~~x~~")).toContain("<del>x</del>");
  });
  it("turns single newlines into <br> (breaks)", () => {
    expect(renderMarkdown("a\nb")).toContain("<br>");
  });
});

describe("renderMarkdown — sanitization", () => {
  it("strips <script> tags", () => {
    const html = renderMarkdown("<script>alert(1)<\/script>");
    expect(html).not.toContain("<script");
  });
  it("neutralizes img onerror", () => {
    const html = renderMarkdown('<img src=x onerror="alert(1)">');
    expect(html).not.toContain("onerror");
  });
  it("drops javascript: link hrefs", () => {
    const html = renderMarkdown("[x](javascript:alert(1))");
    expect(html).not.toContain("javascript:");
  });
  it("drops data: link hrefs", () => {
    const html = renderMarkdown("[x](data:text/html,<script>1<\/script>)");
    expect(html).not.toContain("data:");
  });
  it("keeps safe http links", () => {
    const html = renderMarkdown("[x](https://example.com)");
    expect(html).toContain('href="https://example.com"');
  });
  it("preserves highlighted code (class attrs survive)", () => {
    const html = renderMarkdown("```js\nconst a = 1;\n```");
    expect(html).toContain('class="hljs');
  });
});
