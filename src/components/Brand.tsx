import { ArrowDownToLine } from "lucide-react";

export function Brand() {
  return <div className="flex items-center gap-3"><div className="grid h-10 w-10 place-items-center rounded-[14px] bg-lime text-accentForeground"><ArrowDownToLine size={21} strokeWidth={2.4} /></div><span className="text-[21px] font-bold tracking-[-.04em]">Pully</span></div>;
}
