import { Icon } from "./Icon";
import type { Instruction } from "../types";

const SCOPES = ["chat", "coding", "both"];
const inputCls =
  "w-full rounded-lg border border-neutral-800 bg-neutral-900 px-3 py-2 text-sm text-neutral-200 outline-hidden focus:border-neutral-700";

/**
 * Scoped instruction blocks.
 *
 * Presentational on purpose: it takes the list and hands back a new one, so the
 * settings dialog's form and the Customise hub can each drive it from their own
 * state without either owning the other's.
 */
export function InstructionsSection({
  instructions,
  onChange,
}: {
  instructions: Instruction[];
  onChange: (next: Instruction[]) => void;
}) {
  const patch = (id: string, changes: Partial<Instruction>) =>
    onChange(instructions.map((i) => (i.id === id ? { ...i, ...changes } : i)));

  const add = () =>
    onChange([
      ...instructions,
      {
        id: `custom-${Date.now()}`,
        name: "New instruction",
        scope: "chat",
        body: "",
        enabled: true,
      },
    ]);

  const remove = (id: string) => onChange(instructions.filter((i) => i.id !== id));

  return (
    <>
      <p className="text-xs text-neutral-500">
        Instructions apply only in their scope — so “when writing a document use this type” does
        not clutter every chat.
      </p>
      {instructions.length === 0 && (
        <p className="text-xs text-neutral-600">None yet. Add one below.</p>
      )}
      {instructions.map((instruction) => (
        <div
          key={instruction.id}
          className="space-y-2 rounded-lg border border-neutral-800 bg-neutral-900/40 p-3"
        >
          <div className="flex items-center gap-2">
            <input
              className="flex-1 rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-200 outline-hidden focus:border-neutral-700"
              value={instruction.name}
              onChange={(e) => patch(instruction.id, { name: e.target.value })}
            />
            <select
              className="rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-300 outline-hidden focus:border-neutral-700"
              value={instruction.scope}
              onChange={(e) => patch(instruction.id, { scope: e.target.value })}
            >
              {SCOPES.map((s) => (
                <option key={s} value={s}>
                  {s}
                </option>
              ))}
            </select>
            <label className="flex items-center gap-1 text-[11px] text-neutral-400">
              <input
                type="checkbox"
                checked={instruction.enabled}
                onChange={(e) => patch(instruction.id, { enabled: e.target.checked })}
              />
              on
            </label>
            <button
              onClick={() => remove(instruction.id)}
              className="text-neutral-500 hover:text-red-400"
              title="Remove"
            >
              <Icon name="close" />
            </button>
          </div>
          <textarea
            className={`${inputCls} h-24 resize-y font-mono text-[11px]`}
            value={instruction.body}
            onChange={(e) => patch(instruction.id, { body: e.target.value })}
          />
        </div>
      ))}
      <button
        onClick={add}
        className="rounded-lg px-3 py-1.5 text-[13px] text-neutral-300 transition duration-150 hover:bg-neutral-800 hover:text-neutral-100"
      >
        + Add instruction
      </button>
    </>
  );
}
