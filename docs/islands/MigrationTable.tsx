import { Fragment, useState } from "react";
import mapping from "../generated/migrations.json";

export const client = "load";

/** Lets long IDs wrap after `.`, `/`, `-`, `_` or `\`, and inside long camelCase names. */
function Breakable({ text }: { text: string }) {
  const parts = text.split(/(?<=[./\\_-])/)
    .flatMap(part => part.length > 24 ? part.split(/(?<=[a-z0-9])(?=[A-Z])/) : [part]);
  return <>{parts.map((part, index) => <Fragment key={index}>{index > 0 && <wbr />}{part}</Fragment>)}</>;
}

export default function MigrationTable({ tool }: { tool: keyof typeof mapping.tools }) {
  const [query, setQuery] = useState("");
  const [unsupported, setUnsupported] = useState(false);
  const all = mapping.tools[tool];
  const needle = query.toLowerCase().trim();
  const rows = all.filter(row =>
    (!unsupported || row.unsupported) &&
    `${row.name} ${row.rules.join(" ")} ${row.note}`.toLowerCase().includes(needle)
  );
  return (
    <div className="rule-reference migration-table">
      <div className="migration-toolbar">
        <input type="search" value={query} onChange={event => setQuery(event.target.value)}
          placeholder="Search upstream names and Pun IDs" aria-label="Search upstream names and Pun IDs" />
        <label className="rule-check">
          <input type="checkbox" checked={unsupported} onChange={event => setUnsupported(event.target.checked)} />
          Unsupported only
        </label>
      </div>
      <p role="status" className="migration-count">
        {rows.length.toLocaleString("en-US")} of {all.length.toLocaleString("en-US")} upstream entries
        {unsupported && " · includes dropped rules"}
      </p>
      <div className="blume-table-scroll" tabIndex={0} role="region" aria-label="Rule mapping">
        <table>
          <colgroup><col className="migration-col-upstream" /><col /></colgroup>
          <thead><tr><th>Upstream</th><th>Pun rule</th></tr></thead>
          <tbody>{rows.map(row => <tr key={`${row.tool}:${row.name}`}>
            <td data-label="Upstream">
              <a href={row.url}><code><Breakable text={row.name} /></code></a>
              {row.tool !== tool && <small className="migration-tool">{row.tool}</small>}
            </td>
            <td data-label="Pun rule">
              {row.rules.length
                ? row.rules.map(rule => <a key={rule} href={`/docs/rules/${rule}`}><code><Breakable text={rule} /></code></a>)
                : <span className="migration-badge">Not supported</span>}
              {row.note && <p className="migration-note">{row.note}</p>}
            </td>
          </tr>)}</tbody>
        </table>
        {!rows.length && <p className="migration-empty">No matching upstream rules.</p>}
      </div>
    </div>
  );
}
