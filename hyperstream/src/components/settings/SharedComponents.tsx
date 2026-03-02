import React from "react";

export const Toggle: React.FC<{
  checked: boolean;
  onChange: (val: boolean) => void;
  label?: string;
}> = ({ checked, onChange, label }) => (
  <div
    className="flex items-center justify-between py-2 group cursor-pointer"
    onClick={() => onChange(!checked)}
  >
    {label && (
      <span className="text-sm font-medium text-slate-300 group-hover:text-white transition-colors">
        {label}
      </span>
    )}
    <div
      className={`w-11 h-6 flex items-center rounded-full p-1 duration-300 ease-in-out ${checked ? "bg-blue-600" : "bg-slate-700"}`}
    >
      <div
        className={`bg-white w-4 h-4 rounded-full shadow-md transform duration-300 ease-in-out ${checked ? "translate-x-5" : ""}`}
      />
    </div>
  </div>
);

export const SectionHeader: React.FC<{ icon: any; title: string }> = ({
  icon: Icon,
  title,
}) => (
  <div className="flex items-center gap-2 mb-4 pb-2 border-b border-slate-700/50 text-blue-400">
    <Icon size={18} />
    <h3 className="font-semibold text-sm uppercase tracking-wider">{title}</h3>
  </div>
);
