import { useEffect, useRef } from "react";
import gsap from "gsap";
import { motion } from "motion/react";
import { MarkdownTypewriter } from "markdown-typewriter/react";
import * as THREE from "three";
import * as Tone from "tone";
import PixiPanel from "./PixiPanel.tsx";
import { useFramePacingProbe } from "./framePacing.ts";

const DIALOGUE = `**Alice:** Everything at once: *three.js* scene, *PixiJS* overlay, **motion**,
**GSAP** and a *Tone.js* score, as a visual novel with a 3D stage would run it.

- 3D: lit instanced cubes plus a particle cloud.
- Audio: a looping score with synth, kick, hat and reverb.`;

/**
 * Whole-game workload: every library a game here is likely to combine, running together.
 * three.js draws a lit instanced scene, PixiJS a second canvas, motion/markdown-typewriter and
 * GSAP animate the DOM, Tone.js plays a looping sequence through effects. Audio needs a user
 * gesture in most browsers: it is started at load and again on the first click or key.
 */
export type FixturePart = "three" | "pixi" | "motion" | "gsap" | "tone" | "worker";
export const ALL_PARTS: FixturePart[] = ["three", "pixi", "motion", "gsap", "tone", "worker"];

/** Spins on arithmetic in 6 ms slices, posting a result per slice: a physics/pathfinding-like load. */
const WORKER_SOURCE = `let n = 0; function slice() { const end = performance.now() + 6; let h = 1; while (performance.now() < end) { for (let i = 0; i < 2000; i++) h = Math.imul(h ^ i, 16777619); n++; } postMessage(h); setTimeout(slice, 10); } slice();`;

export default function FullStackFixture({ parts = ALL_PARTS, name = "full-stack" }: { parts?: FixturePart[]; name?: string }) {
  useFramePacingProbe(name);
  const on = (part: FixturePart) => parts.includes(part);
  const threeRef = useRef<HTMLDivElement>(null);
  const gsapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = threeRef.current;
    if (!container || !on("three")) return;
    const width = 640;
    const height = 360;
    const scene = new THREE.Scene();
    scene.fog = new THREE.Fog(0x050510, 8, 26);
    const camera = new THREE.PerspectiveCamera(55, width / height, 0.1, 60);
    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(1);
    renderer.setSize(width, height);
    container.appendChild(renderer.domElement);

    scene.add(new THREE.AmbientLight(0x404060, 1.2));
    const sun = new THREE.DirectionalLight(0xffffff, 2);
    sun.position.set(5, 8, 4);
    scene.add(sun);
    const lamps = [0xff5577, 0x55aaff].map((color) => {
      const light = new THREE.PointLight(color, 40, 12);
      scene.add(light);
      return light;
    });

    const count = 400;
    const cubes = new THREE.InstancedMesh(
      new THREE.BoxGeometry(0.5, 0.5, 0.5),
      new THREE.MeshStandardMaterial({ roughness: 0.4, metalness: 0.3 }),
      count,
    );
    const color = new THREE.Color();
    for (let i = 0; i < count; i++) cubes.setColorAt(i, color.setHSL(i / count, 0.7, 0.55));
    scene.add(cubes);

    const particleCount = 4000;
    const positions = new Float32Array(particleCount * 3);
    for (let i = 0; i < positions.length; i++) positions[i] = (Math.random() - 0.5) * 24;
    const particleGeometry = new THREE.BufferGeometry();
    particleGeometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    const particles = new THREE.Points(particleGeometry, new THREE.PointsMaterial({ size: 0.05, color: 0x99ccff }));
    scene.add(particles);

    const matrix = new THREE.Matrix4();
    const rotation = new THREE.Euler();
    const quaternion = new THREE.Quaternion();
    const one = new THREE.Vector3(1, 1, 1);
    const position = new THREE.Vector3();
    let frame = 0;
    const animate = (time: number) => {
      const t = time / 1000;
      for (let i = 0; i < count; i++) {
        const a = (i / count) * Math.PI * 12 + t * 0.3;
        const radius = 2 + (i % 20) * 0.3;
        position.set(Math.cos(a) * radius, Math.sin(a * 0.5 + i) * 1.5, Math.sin(a) * radius);
        rotation.set(t + i, t * 0.7 + i, 0);
        matrix.compose(position, quaternion.setFromEuler(rotation), one);
        cubes.setMatrixAt(i, matrix);
      }
      cubes.instanceMatrix.needsUpdate = true;
      particles.rotation.y = t * 0.05;
      lamps[0].position.set(Math.cos(t) * 5, 2, Math.sin(t) * 5);
      lamps[1].position.set(Math.sin(t * 0.7) * 5, -2, Math.cos(t * 0.7) * 5);
      camera.position.set(Math.cos(t * 0.1) * 9, 3, Math.sin(t * 0.1) * 9);
      camera.lookAt(0, 0, 0);
      renderer.render(scene, camera);
      frame = requestAnimationFrame(animate);
    };
    frame = requestAnimationFrame(animate);
    return () => {
      cancelAnimationFrame(frame);
      cubes.geometry.dispose();
      (cubes.material as THREE.Material).dispose();
      particleGeometry.dispose();
      renderer.dispose();
      renderer.domElement.remove();
    };
  }, []);

  useEffect(() => {
    if (!on("gsap")) return;
    const tween = gsap.to(gsapRef.current, { rotation: 360, x: 200, duration: 4, repeat: -1, yoyo: true, ease: "power2.inOut" });
    return () => {
      tween.kill();
    };
  }, []);

  useEffect(() => {
    if (!on("worker")) return;
    const url = URL.createObjectURL(new Blob([WORKER_SOURCE], { type: "text/javascript" }));
    const worker = new Worker(url);
    let messages = 0;
    worker.onmessage = () => {
      messages++;
    };
    const report = setInterval(() => console.log("WORKER messages=" + messages), 10000);
    return () => {
      clearInterval(report);
      worker.terminate();
      URL.revokeObjectURL(url);
    };
  }, []);

  useEffect(() => {
    if (!on("tone")) return;
    // `?tone=poly,membrane` (default `all`) keeps only some pieces: used to bisect audio cost.
    const wanted = (new URLSearchParams(location.search).get("tone") ?? "all").split(",");
    const want = (name: string) => wanted.includes("all") || wanted.includes(name);
    const ok: string[] = [];
    const failed: string[] = [];
    const disposables: { dispose(): unknown }[] = [];
    // A game would hit a missing WebAudio node as an exception: build every piece on its own so the
    // rest of the page (and the measurement) survives, and report what this engine could not create.
    const piece = <T extends { dispose(): unknown }>(name: string, make: () => T): T | undefined => {
      if (!name.startsWith("Sequence") && !want(name.toLowerCase())) return undefined;
      try {
        const value = make();
        disposables.push(value);
        ok.push(name);
        return value;
      } catch (error) {
        failed.push(name + ": " + String(error));
        return undefined;
      }
    };
    const reverb = piece("Reverb", () => new Tone.Reverb({ decay: 3, wet: 0.3 }).toDestination());
    const delay = piece("FeedbackDelay", () => new Tone.FeedbackDelay("8n", 0.25).connect(reverb ?? Tone.getDestination()));
    const lead = piece("PolySynth", () => new Tone.PolySynth(Tone.Synth, { volume: -14 }).connect(delay ?? Tone.getDestination()));
    const kick = piece("MembraneSynth", () => new Tone.MembraneSynth({ volume: -10 }).toDestination());
    const hat = piece("NoiseSynth", () => new Tone.NoiseSynth({ volume: -24, envelope: { attack: 0.001, decay: 0.05, sustain: 0 } }).toDestination());
    console.log("TONEINFO " + JSON.stringify({ ok, failed }));
    const notes = ["C4", "E4", "G4", "B4", "A4", "G4", "E4", "D4"];
    const melody = piece("Sequence(melody)", () => new Tone.Sequence((time, note) => lead?.triggerAttackRelease(note, "8n", time), notes, "8n"));
    const drums = piece(
      "Sequence(drums)",
      () =>
        new Tone.Sequence(
          (time, step) => {
            if (step % 4 === 0) kick?.triggerAttackRelease("C1", "8n", time);
            hat?.triggerAttackRelease("32n", time);
          },
          Array.from({ length: 8 }, (_, i) => i),
          "8n",
        ),
    );
    const transport = Tone.getTransport();
    transport.bpm.value = 120;
    melody?.start(0);
    drums?.start(0);
    const start = () => {
      Tone.start()
        .then(() => {
          if (transport.state !== "started") transport.start();
          console.log("AUDIOSTART " + Tone.getContext().state);
        })
        .catch((error) => console.log("TONE failed " + String(error)));
    };
    start();
    const stateLog = setInterval(() => console.log("AUDIOSTATE " + Tone.getContext().state + " transport=" + transport.state), 15000);
    window.addEventListener("pointerdown", start);
    window.addEventListener("keydown", start);
    return () => {
      window.removeEventListener("pointerdown", start);
      window.removeEventListener("keydown", start);
      clearInterval(stateLog);
      transport.stop();
      for (const node of disposables) node.dispose();
    };
  }, []);

  return (
    <main data-performance-fixture={name} style={{ position: "relative", minHeight: "100vh", background: "#000", color: "#eee" }}>
      <div ref={threeRef} style={{ position: "absolute", inset: 0, display: "grid", placeItems: "center" }} />
      {on("pixi") && (
        <div style={{ position: "absolute", top: 8, right: 8, opacity: 0.9 }}>
          <PixiPanel animated diagnostics={false} />
        </div>
      )}
      {on("motion") && <motion.div
        animate={{ x: [0, 300, 0], rotate: [0, 180, 360] }}
        transition={{ duration: 3, repeat: Infinity, ease: "easeInOut" }}
        style={{ position: "absolute", top: 24, left: 24, width: 48, height: 48, background: "#8ab4f8" }}
      />}
      {on("gsap") && <div ref={gsapRef} style={{ position: "absolute", top: 100, left: 24, width: 40, height: 40, background: "#f8b48a" }} />}
      {on("motion") && <div
        style={{
          position: "absolute",
          left: "10%",
          right: "10%",
          bottom: 24,
          padding: "1rem 1.5rem",
          background: "rgba(0,0,0,0.7)",
          borderRadius: 12,
          fontFamily: "sans-serif",
          fontSize: 18,
          minHeight: 140,
        }}
      >
        <MarkdownTypewriter delay={20}>{DIALOGUE}</MarkdownTypewriter>
      </div>}
    </main>
  );
}
