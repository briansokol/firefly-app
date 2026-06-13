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
