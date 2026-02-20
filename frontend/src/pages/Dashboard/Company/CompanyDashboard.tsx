import { ShipmentVolumeChart } from "../../../components/dashboard/Charts/ShipmentVolumeChart";

export default function CompanyDashboard() {
  return (
    <div className="min-h-screen bg-gray-50">
      <div className="mx-auto max-w-7xl px-4 py-8 sm:px-6 lg:px-8">
        <h1 className="mb-8 text-2xl font-bold text-gray-900">
          Company Dashboard
        </h1>

        <div className="grid gap-6">
          <ShipmentVolumeChart />
        </div>
      </div>
    </div>
  );
}
