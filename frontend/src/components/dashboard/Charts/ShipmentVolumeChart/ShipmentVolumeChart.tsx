import { useState, useMemo } from "react";
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
} from "recharts";
import { generateMockShipmentData } from "../../../../data/mockShipmentData";

type Range = 7 | 30 | 90;

const RANGE_OPTIONS: { label: string; value: Range }[] = [
  { label: "7D", value: 7 },
  { label: "30D", value: 30 },
  { label: "90D", value: 90 },
];

const XAXIS_INTERVAL: Record<Range, number> = {
  7: 0,
  30: 4,
  90: 13,
};

export default function ShipmentVolumeChart() {
  const [range, setRange] = useState<Range>(30);

  const data = useMemo(() => generateMockShipmentData(range), [range]);

  return (
    <div className="rounded-2xl border border-gray-200 bg-white p-6 shadow-sm">
      <div className="mb-4 flex items-center justify-between">
        <h3 className="text-lg font-semibold text-gray-900">
          Shipment Volume
        </h3>

        <div className="flex gap-1 rounded-lg bg-gray-100 p-1">
          {RANGE_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              onClick={() => setRange(opt.value)}
              className={`rounded-md px-3 py-1 text-sm font-medium transition-colors ${
                range === opt.value
                  ? "bg-blue-600 text-white shadow-sm"
                  : "text-gray-600 hover:text-gray-900"
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>

      <ResponsiveContainer width="100%" height={320}>
        <AreaChart data={data}>
          <defs>
            <linearGradient id="colorCount" x1="0" y1="0" x2="0" y2="1">
              <stop offset="5%" stopColor="#2563eb" stopOpacity={0.3} />
              <stop offset="95%" stopColor="#2563eb" stopOpacity={0} />
            </linearGradient>
          </defs>

          <CartesianGrid strokeDasharray="3 3" stroke="#e5e7eb" />

          <XAxis
            dataKey="date"
            interval={XAXIS_INTERVAL[range]}
            tick={{ fontSize: 12, fill: "#6b7280" }}
            tickLine={false}
            axisLine={{ stroke: "#e5e7eb" }}
          />

          <YAxis
            tick={{ fontSize: 12, fill: "#6b7280" }}
            tickLine={false}
            axisLine={false}
            width={40}
          />

          <Tooltip
            contentStyle={{
              borderRadius: "0.5rem",
              border: "1px solid #e5e7eb",
              boxShadow: "0 4px 6px -1px rgb(0 0 0 / 0.1)",
            }}
            formatter={(value: number | string | undefined) => [
              value ?? 0,
              "Shipments",
            ]}
          />

          <Area
            type="monotone"
            dataKey="count"
            stroke="#2563eb"
            strokeWidth={2}
            fill="url(#colorCount)"
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
