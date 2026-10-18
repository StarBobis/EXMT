<script setup lang="ts">
/**
 * three.js preview of a whole skin.
 *
 * Every mesh of the selected skin is rendered together in one scene (bind pose,
 * same coordinate space), so the character is complete instead of one part at a
 * time. Geometry arrives as typed arrays from the Python core.
 */
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import type { PreviewGeometry } from "../lib/preview";

const props = defineProps<{
  meshes: PreviewGeometry[];
  hidden: string[];
  selected: string[];
  loading?: boolean;
}>();

const host = ref<HTMLDivElement | null>(null);

let renderer: THREE.WebGLRenderer | null = null;
let scene: THREE.Scene | null = null;
let camera: THREE.PerspectiveCamera | null = null;
let controls: OrbitControls | null = null;
let observer: ResizeObserver | null = null;
let group: THREE.Group | null = null;
let grid: THREE.GridHelper | null = null;
const built = new Map<string, THREE.Mesh>();

const COLOR_PICKED = 0x8fc9ff;
const COLOR_IDLE = 0x5f6a78;

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

function clearGroup() {
  built.clear();
  if (!scene || !group) return;
  scene.remove(group);
  group.traverse((obj) => {
    const mesh = obj as THREE.Mesh;
    if (mesh.geometry) mesh.geometry.dispose();
    const material = mesh.material as THREE.Material | THREE.Material[] | undefined;
    if (Array.isArray(material)) material.forEach((m) => m.dispose());
    else material?.dispose();
  });
  group = null;
}

function fit(all: PreviewGeometry[]) {
  if (!camera || !controls) return;
  const min = new THREE.Vector3(Infinity, Infinity, Infinity);
  const max = new THREE.Vector3(-Infinity, -Infinity, -Infinity);
  for (const geo of all) {
    min.min(new THREE.Vector3(geo.bounds.min[0], geo.bounds.min[1], geo.bounds.min[2]));
    max.max(new THREE.Vector3(geo.bounds.max[0], geo.bounds.max[1], geo.bounds.max[2]));
  }
  if (!Number.isFinite(min.x)) return;

  const size = max.clone().sub(min);
  const center = min.clone().add(max).multiplyScalar(0.5);
  const maxDim = Math.max(size.x, size.y, size.z, 0.001);
  const fov = (camera.fov * Math.PI) / 180;
  // fit the largest dimension into the vertical field of view, with margin
  const distance = (maxDim / 2 / Math.tan(fov / 2)) * 1.35;
  const direction = new THREE.Vector3(0.45, 0.28, 1).normalize();

  controls.target.copy(center);
  camera.position.copy(center).addScaledVector(direction, distance);
  camera.near = Math.max(distance / 200, 0.001);
  camera.far = distance * 6 + maxDim * 4;
  camera.updateProjectionMatrix();
  controls.update();

  if (grid) {
    scene?.remove(grid);
    grid.geometry.dispose();
    (grid.material as THREE.Material).dispose();
  }
  grid = new THREE.GridHelper(Math.max(maxDim * 4, 4), 20, 0x2c3a4a, 0x1b242e);
  grid.position.set(center.x, min.y, center.z);
  scene?.add(grid);
}

function build(all: PreviewGeometry[]) {
  if (!scene) return;
  clearGroup();
  if (!all.length) {
    render();
    return;
  }

  group = new THREE.Group();
  for (const geo of all) {
    if (!geo.positions.length || !geo.indices.length) continue;
    const buffer = new THREE.BufferGeometry();
    buffer.setAttribute("position", new THREE.BufferAttribute(geo.positions, 3));
    if (geo.normals) buffer.setAttribute("normal", new THREE.BufferAttribute(geo.normals, 3));
    if (geo.uvs) buffer.setAttribute("uv", new THREE.BufferAttribute(geo.uvs, 2));
    buffer.setIndex(new THREE.BufferAttribute(geo.indices, 1));
    if (!geo.normals) buffer.computeVertexNormals();
    buffer.computeBoundingSphere();

    const material = new THREE.MeshStandardMaterial({
      color: props.selected.includes(geo.name) ? COLOR_PICKED : COLOR_IDLE,
      metalness: 0.12,
      roughness: 0.7,
      side: THREE.DoubleSide,
    });
    const mesh = new THREE.Mesh(buffer, material);
    mesh.name = geo.name;
    mesh.visible = !props.hidden.includes(geo.name);
    built.set(geo.name, mesh);
    group.add(mesh);
  }
  scene.add(group);
  fit(all);
  render();
}

function syncVisibility() {
  for (const [name, mesh] of built) mesh.visible = !props.hidden.includes(name);
  render();
}

/** 勾选的网格亮蓝，没勾的压成灰色——和列表里的勾选状态一致。 */
function syncColors() {
  for (const [name, mesh] of built) {
    const material = mesh.material as THREE.MeshStandardMaterial;
    const picked = props.selected.includes(name);
    material.color.setHex(picked ? COLOR_PICKED : COLOR_IDLE);
    material.emissive.setHex(picked ? 0x0d2436 : 0x000000);
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

  if (props.meshes.length) build(props.meshes);
});

watch(
  () => props.meshes,
  (all) => build(all),
);
watch(() => props.hidden, syncVisibility, { deep: true });
watch(() => props.selected, syncColors, { deep: true });

onBeforeUnmount(() => {
  observer?.disconnect();
  controls?.dispose();
  clearGroup();
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
    <div v-if="!meshes.length && !loading" class="meshviewer__empty">3D</div>
    <div v-if="loading" class="meshviewer__loading"><span class="spinner" /></div>
  </div>
</template>

<style scoped>
.meshviewer {
  position: relative;
  width: 100%;
  height: 100%;
  min-height: 220px;
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
