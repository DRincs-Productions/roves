import { lazy, StrictMode, Suspense, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import PerformanceFixture, { performanceFixtureMode, type PerformanceFixtureMode } from "./PerformanceFixture.tsx";
import "./index.css";

const App = lazy(() => import("./App.tsx"));
function DiagnosticsRoot() {
  const [mode, setMode] = useState(() => performanceFixtureMode(window.location.search));
  const [fromMenu, setFromMenu] = useState(false);
  useEffect(() => {
    const restore = () => {
      setMode(performanceFixtureMode(window.location.search));
      setFromMenu(history.state?.rovesPerformanceFixture === true);
    };
    window.addEventListener("popstate", restore);
    return () => window.removeEventListener("popstate", restore);
  }, []);
  const navigate = (next: PerformanceFixtureMode | null) => {
    const url = new URL(window.location.href);
    if (next) url.searchParams.set("perf", next);
    else url.searchParams.delete("perf");
    history.pushState({ rovesPerformanceFixture: true }, "", url);
    setMode(next);
    setFromMenu(true);
  };
  return mode ? (
    <>
      {/* Direct ?perf= launches keep the original measurement scene unchanged. */}
      {fromMenu && <button style={{ position: "fixed", top: 8, left: 8, zIndex: 1000 }}
        onClick={() => navigate(null)}>Back to diagnostics</button>}
      <PerformanceFixture key={mode} mode={mode} />
    </>
  ) : (
    <Suspense fallback={<p>Loading diagnostics…</p>}>
      <App onPerformanceNavigate={navigate} />
    </Suspense>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <DiagnosticsRoot />
  </StrictMode>,
);
