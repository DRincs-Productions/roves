import { lazy, StrictMode, Suspense } from "react";
import { createRoot } from "react-dom/client";
import PerformanceFixture, { performanceFixtureMode } from "./PerformanceFixture.tsx";
import "./index.css";

const App = lazy(() => import("./App.tsx"));
const performanceMode = performanceFixtureMode(window.location.search);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {performanceMode ? (
      <PerformanceFixture mode={performanceMode} />
    ) : (
      <Suspense fallback={<p>Loading diagnostics…</p>}>
        <App />
      </Suspense>
    )}
  </StrictMode>,
);
