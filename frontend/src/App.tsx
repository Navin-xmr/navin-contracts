import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import CompanyDashboard from "./pages/Dashboard/Company/CompanyDashboard";

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/dashboard/company" element={<CompanyDashboard />} />
        <Route path="*" element={<Navigate to="/dashboard/company" replace />} />
      </Routes>
    </BrowserRouter>
  );
}

export default App;
