import { useEffect, useRef, useState } from "react";
import * as THREE from "three";

/**
 * Minimal Three.js smoke test: a plain WebGL renderer (no framework glue) with
 * a spinning cube. Exercises the same low-level WebGL context creation PixiJS
 * also depends on, but through an entirely separate library — useful to tell
 * apart "WebGL itself is broken in this Servo build" from "something specific
 * to PixiJS is broken".
 */
export default function ThreePanel() {
  const containerRef = useRef<HTMLDivElement>(null);
  const [status, setStatus] = useState("Not started.");

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let renderer: THREE.WebGLRenderer | undefined;
    let frameId: number | undefined;

    try {
      const width = 300;
      const height = 200;

      const scene = new THREE.Scene();
      const camera = new THREE.PerspectiveCamera(50, width / height, 0.1, 10);
      camera.position.z = 3;

      renderer = new THREE.WebGLRenderer({ antialias: true });
      renderer.setSize(width, height);
      container.appendChild(renderer.domElement);

      const cube = new THREE.Mesh(
        new THREE.BoxGeometry(1, 1, 1),
        new THREE.MeshNormalMaterial(),
      );
      scene.add(cube);

      const animate = () => {
        cube.rotation.x += 0.02;
        cube.rotation.y += 0.03;
        renderer!.render(scene, camera);
        frameId = requestAnimationFrame(animate);
      };
      animate();

      setStatus("ok — WebGLRenderer created, render loop running");
    } catch (error) {
      setStatus(`FAILED — ${String(error)}`);
    }

    return () => {
      if (frameId !== undefined) cancelAnimationFrame(frameId);
      renderer?.dispose();
    };
  }, []);

  return (
    <div>
      <p>Three.js: {status}</p>
      <div ref={containerRef} style={{ width: 300, height: 200 }} />
    </div>
  );
}
