/** The control that fetches the next older page of a listing. */
export function More({
  onClick,
  loading,
  what,
}: {
  readonly onClick: () => void;
  readonly loading: boolean;
  readonly what: string;
}) {
  return (
    <div className="border-t border-mesh-border px-4 py-3">
      <button
        type="button"
        onClick={onClick}
        disabled={loading}
        className="rounded-md border border-mesh-border px-3 py-1.5 text-sm text-mesh-text hover:bg-mesh-raised disabled:text-mesh-faint focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
      >
        {loading ? 'wird geladen …' : `Ältere ${what} laden`}
      </button>
    </div>
  );
}
