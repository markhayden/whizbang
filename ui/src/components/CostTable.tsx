import { dollars } from "../format";

export interface CostLine {
  /** The capability and how often: "fill × 2". */
  what: string;
  /** What served it: "FAL", "SAM3 local". */
  backend: string;
  /** US dollars; 0 for free local work. */
  cost: number;
}

export interface CostTableProps {
  lines: CostLine[];
}

/** The total of the lines, in US dollars. */
export function costTotal(lines: CostLine[]): number {
  return lines.reduce((sum, line) => sum + line.cost, 0);
}

/** A run's cost split by capability and backend, free local work shown as free. */
export function CostTable(props: CostTableProps) {
  return (
    <table class="cost-table mono">
      <caption class="visually-hidden">Cost</caption>
      <tbody>
        {props.lines.map((line, index) => (
          <tr key={index}>
            <th scope="row">
              {line.what} · {line.backend}
            </th>
            <td>{line.cost === 0 ? "free" : dollars(line.cost)}</td>
          </tr>
        ))}
      </tbody>
      <tfoot>
        <tr>
          <th scope="row">total</th>
          <td>{dollars(costTotal(props.lines))}</td>
        </tr>
      </tfoot>
    </table>
  );
}
