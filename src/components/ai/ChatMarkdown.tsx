// Renders model output as Markdown, built from marked's tokens as React elements.
// Nothing is ever parsed as HTML: raw HTML shows as text, images aren't loaded
// (their address is shown instead), and only web and email links can be opened,
// with a click and their destination in view (ARCHITECTURE §10.4).
import { Lexer, type Token, type Tokens } from "marked";
import { Fragment, type ReactNode, useMemo } from "react";
import { ipc } from "@/lib/ipc/client";

const OPENABLE = /^(https?:|mailto:)/i;

function Inline({ tokens }: { tokens: Token[] | undefined }) {
  return <>{(tokens ?? []).map((token, i) => inline(token, i))}</>;
}

function inline(token: Token, key: number): ReactNode {
  switch (token.type) {
    case "text":
      return "tokens" in token && token.tokens ? (
        <Inline key={key} tokens={token.tokens} />
      ) : (
        <Fragment key={key}>{token.text}</Fragment>
      );
    case "escape":
      return <Fragment key={key}>{token.text}</Fragment>;
    case "strong":
      return (
        <strong key={key}>
          <Inline tokens={token.tokens} />
        </strong>
      );
    case "em":
      return (
        <em key={key}>
          <Inline tokens={token.tokens} />
        </em>
      );
    case "del":
      return (
        <del key={key}>
          <Inline tokens={token.tokens} />
        </del>
      );
    case "codespan":
      return (
        <code key={key} className="rounded-sm bg-muted px-1 py-px font-mono text-[12.5px]">
          {token.text}
        </code>
      );
    case "br":
      return <br key={key} />;
    case "link":
      return <ChatLink key={key} token={token as Tokens.Link} />;
    case "image":
      return (
        <span key={key} className="text-muted-foreground">
          [Image{token.text ? `: ${token.text}` : ""} · {token.href}]
        </span>
      );
    case "checkbox":
      return (
        <input
          key={key}
          type="checkbox"
          checked={token.checked}
          disabled
          aria-label={token.checked ? "Done" : "Not done"}
          className="mr-1.5 align-middle"
        />
      );
    default:
      // Raw HTML and anything newer than this renderer: as plain text.
      return <Fragment key={key}>{token.raw}</Fragment>;
  }
}

function ChatLink({ token }: { token: Tokens.Link }) {
  if (!OPENABLE.test(token.href)) {
    return (
      <span>
        <Inline tokens={token.tokens} />{" "}
        <span className="text-muted-foreground">({token.href})</span>
      </span>
    );
  }
  return (
    <button
      type="button"
      title={`Open ${token.href}`}
      onClick={() => void ipc.openExternal(token.href).catch(() => {})}
      className="cursor-pointer text-link underline decoration-1 underline-offset-2"
    >
      <Inline tokens={token.tokens} />
    </button>
  );
}

function Blocks({ tokens }: { tokens: Token[] }) {
  return <>{tokens.map((token, i) => block(token, i))}</>;
}

function block(token: Token, key: number): ReactNode {
  switch (token.type) {
    case "space":
    case "def":
      return null;
    case "heading":
      return (
        <p key={key} className="mt-3 font-semibold first:mt-0">
          <Inline tokens={token.tokens} />
        </p>
      );
    case "paragraph":
      return (
        <p key={key} className="mt-2 first:mt-0">
          <Inline tokens={token.tokens} />
        </p>
      );
    case "text":
      return "tokens" in token && token.tokens ? (
        <Inline key={key} tokens={token.tokens} />
      ) : (
        <Fragment key={key}>{token.text}</Fragment>
      );
    case "code":
      return (
        <pre
          key={key}
          className="selectable mt-2 overflow-x-auto rounded-md bg-muted px-3 py-2 font-mono text-[12.5px] leading-5 first:mt-0"
        >
          <code>{token.text}</code>
        </pre>
      );
    case "blockquote":
      return (
        <blockquote key={key} className="mt-2 border-l-2 pl-3 text-muted-foreground first:mt-0">
          <Blocks tokens={token.tokens ?? []} />
        </blockquote>
      );
    case "list": {
      const list = token as Tokens.List;
      const items = list.items.map((item, i) => (
        // Items have no IDs and never move within a rendered answer.
        // biome-ignore lint/suspicious/noArrayIndexKey: see above
        <li key={i} className="mt-0.5">
          <Blocks tokens={item.tokens} />
        </li>
      ));
      return list.ordered ? (
        <ol key={key} start={Number(list.start) || 1} className="mt-2 list-decimal pl-5 first:mt-0">
          {items}
        </ol>
      ) : (
        <ul key={key} className="mt-2 list-disc pl-5 first:mt-0">
          {items}
        </ul>
      );
    }
    case "table": {
      const table = token as Tokens.Table;
      return (
        <div key={key} className="mt-2 overflow-x-auto first:mt-0">
          <table className="text-[12.5px]">
            <thead>
              <tr>
                {table.header.map((cell, i) => (
                  // biome-ignore lint/suspicious/noArrayIndexKey: columns are positional
                  <th key={i} className="border-b px-2 py-1 text-left font-medium">
                    <Inline tokens={cell.tokens} />
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {table.rows.map((row, r) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: rows are positional
                <tr key={r}>
                  {row.map((cell, c) => (
                    // biome-ignore lint/suspicious/noArrayIndexKey: columns are positional
                    <td key={c} className="border-b px-2 py-1 align-top">
                      <Inline tokens={cell.tokens} />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    }
    case "hr":
      return <hr key={key} className="my-3" />;
    // A task list item starts with its checkbox.
    case "checkbox":
      return inline(token, key);
    default:
      // Raw HTML blocks and unknown tokens: as plain text.
      return (
        <p key={key} className="mt-2 whitespace-pre-wrap first:mt-0">
          {token.raw}
        </p>
      );
  }
}

export function ChatMarkdown({ text }: { text: string }) {
  const tokens = useMemo(() => Lexer.lex(text, { gfm: true }), [text]);
  return (
    <div className="selectable text-[13.5px] leading-[1.6] break-words">
      <Blocks tokens={tokens} />
    </div>
  );
}
