export interface ShipmentDataPoint {
  date: string;
  count: number;
}

export function generateMockShipmentData(days: number): ShipmentDataPoint[] {
  const data: ShipmentDataPoint[] = [];
  const today = new Date();

  for (let i = days - 1; i >= 0; i--) {
    const date = new Date(today);
    date.setDate(today.getDate() - i);

    const base = 20 + Math.sin(i * 0.3) * 10;
    const noise = Math.floor(Math.random() * 8) - 4;
    const count = Math.max(0, Math.round(base + noise));

    data.push({
      date: date.toLocaleDateString("en-US", {
        month: "short",
        day: "numeric",
      }),
      count,
    });
  }

  return data;
}
