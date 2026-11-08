import mark from "../assets/pully-mark.png";

/** The generated mark is masked so it follows the selected accent foreground. */
export function PullyMark({ className = "" }: { className?: string }) {
  return <span aria-hidden="true" className={`pully-mark ${className}`} style={{ maskImage: `url(${mark})`, WebkitMaskImage: `url(${mark})` }}/>;
}

export function Brand() {
  return <div className="flex items-center gap-3"><div className="grid h-10 w-10 place-items-center rounded-[14px] bg-lime text-accentForeground"><PullyMark className="h-8 w-8"/></div><span className="text-[21px] font-bold tracking-[-.04em]">Pully</span></div>;
}
