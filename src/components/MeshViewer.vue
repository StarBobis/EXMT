<script setup lang="ts">
/**
 * three.js preview of one GF2 mesh.
 *
 * The geometry arrives as typed arrays from the Python core, already mirrored
 * into three's right-handed space; nothing here touches the game files.
 */
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import type { GeometryData } from "../lib/preview";

const props = defineProps<{ geometry: GeometryData | null; loading?: boolean }>();

const host = ref<HTMLDivElement | null>(null);

let renderer: THREE.WebGLRenderer | null = null;
let scene: THREE.Scene | null = null;
let camera: THREE.PerspectiveCamera | null = null;
let controls: OrbitControls | null = null;
let current: THREE.Mesh | null = null;
let grid: THREE.GridHelper | null = null;
let observer: ResizeObserver | null = null;

function render() {
  if (!renderer || !scene || !camera) return;
  renderer.render(scene, camera);
}

function resize() {
  const el = host.value;
  if (!el || !renderer || !camera) return;
  const width = Math.max(1, el.clientWidth);
  const height = Math.max(1, el.clientHeight);
  renderer.setSize(width, height, false);
  camera.aspect = width / height;
  camera.updateProjectionMatrix();
  render();
}

function disposeMesh() {
  if (!scene || !current) return;
  scene.remove(current);
  current.geometry.dispose();
  (current.material as THREE.Material).dispose();
  current = null;
}

function show(geo: GeometryData) {
  if (!scene || !camera || !controls) return;
  disposeMesh();
  if (grid) {
    scene.remove(grid);
    grid.geometry.dispose();
    (grid.material as THREE.Material).dispose();
    grid = null;
  }

  const buffer = new THREE.BufferGeometry();
  buffer.setAttribute("position", new THREE.BufferAttribute(geo.positions, 3));
  if (geo.normals) buffer.setAttribute("normal", new THREE.BufferAttribute(geo.normals, 3));
  if (geo.uvs) buffer.setAttribute("uv", new THREE.BufferAttribute(geo.uvs, 2));
  buffer.setIndex(new THREE.BufferAttribute(geo.indices, 1));
  if (!geo.normals) buffer.computeVertexNormals();
  buffer.computeBoundingSphere();

  const material = new THREE.MeshStandardMaterial({
    color: 0x9fb6cc,
    metalness: 0.12,
    roughness: 0.72,
    side: THREE.DoubleSide,
  });
  current = new THREE.Mesh(buffer, material);
  scene.add(current);

  const sphere = buffer.boundingSphere;
  if (sphere) {
    const radius = Math.max(sphere.radius, 0.001);
    const size = new THREE.Vector3(
      geo.bounds.max[0] - geo.bounds.min[0],
      geo.bounds.max[1] - geo.bounds.min[1],
      geo.bounds.max[2] - geo.bounds.min[2],
    );
    const maxDim = Math.max(size.x, size.y, size.z, 0.001);
    const fov = (camera.fov * Math.PI) / 180;
    // fit the largest dimension into the vertical field of view, with margin
    const distance = (maxDim / 2 / Math.tan(fov / 2)) * 1.3;
    const direction = new THREE.Vector3(0.5, 0.32, 1).normalize();

    controls.target.copy(sphere.center);
    camera.position.copy(sphere.center).addScaledVector(direction, distance);
    camera.near = Math.max(distance / 200, 0.001);
    camera.far = distance * 6 + maxDim * 4;
    camera.updateProjectionMatrix();
    controls.update();

    const floor = geo.bounds?.min?.[1] ?? sphere.center.y - radius;
    const span = Math.max(radius * 4, 4);
    grid = new THREE.GridHelper(span, 20, 0x2c3a4a, 0x1b242e);
    grid.position.set(sphere.center.x, floor, sphere.center.z);
    scene.add(grid);
  }
  render();
}

onMounted(() => {
  const el = host.value;
  if (!el) return;

  renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.domElement.className = "meshviewer__canvas";
  el.appendChild(renderer.domElement);

  scene = new THREE.Scene();
  camera = new THREE.PerspectiveCamera(40, 1, 0.01, 1000);
  camera.position.set(0, 1, 3);

  scene.add(new THREE.HemisphereLight(0xdfe9ff, 0x1a2230, 1.35));
  const key = new THREE.DirectionalLight(0xffffff, 2.1);
  key.position.set(3, 6, 4);
  scene.add(key);
  const rim = new THREE.DirectionalLight(0x7fb8ff, 1.1);
  rim.position.set(-4, 2, -5);
  scene.add(rim);

  controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.addEventListener("change", render);

  observer = new ResizeObserver(resize);
  observer.observe(el);
  resize();

  if (props.geometry) show(props.geometry);
});

watch(
  () => props.geometry,
  (geo) => {
    if (geo) show(geo);
  },
);

onBeforeUnmount(() => {
  observer?.disconnect();
  controls?.dispose();
  renderer?.dispose();
  renderer?.domElement.remove();
  renderer = null;
  scene = null;
  camera = null;
  controls = null;
});
</script>

<template>
  <div ref="host" class="meshviewer">
    <div v-if="!geometry && !loading" class="meshviewer__empty">3D</div>
    <div v-if="loading" class="meshviewer__loading"><span class="spinner" /></div>
  </div>
</template>

<style scoped>
.meshviewer {
  position: relative;
  width: 100%;
  height: 100%;
  min-height: 240px;
  border-radius: 10px;
  overflow: hidden;
  background:
    radial-gradient(120% 90% at 50% 0%, rgba(76, 194, 255, 0.08), transparent 60%),
    var(--card-bg);
  border: 1px solid var(--stroke);
}

.meshviewer :deep(canvas) {
  display: block;
  width: 100% !important;
  height: 100% !important;
}

.meshviewer__empty {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  font-size: 12px;
  letter-spacing: 0.2em;
  color: var(--text-3);
  opacity: 0.6;
}

.meshviewer__loading {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  background: rgba(8, 12, 18, 0.35);
}

.spinner {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  border: 2px solid rgba(255, 255, 255, 0.16);
  border-top-color: var(--accent, #4cc2ff);
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
