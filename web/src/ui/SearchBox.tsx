/**
 * One search field, wherever something can be searched.
 *
 * No submit button and no form: the list narrows while typing, so a button
 * would only confirm what already happened. Clearing it is one keystroke away
 * with the field's own clear control, which is why there is none of ours.
 */
export function SearchBox({
  value,
  onChange,
  label,
  placeholder,
}: {
  readonly value: string;
  readonly onChange: (value: string) => void;
  readonly label: string;
  readonly placeholder: string;
}) {
  return (
    <input
      type="search"
      value={value}
      onChange={(event) => onChange(event.target.value)}
      aria-label={label}
      placeholder={placeholder}
      className="w-full max-w-xs rounded-md border border-mesh-border bg-mesh-bg px-2.5 py-1 text-sm text-mesh-text placeholder:text-mesh-faint focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
    />
  );
}
