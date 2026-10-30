import * as L from "leaflet";
import {simplifyStroke} from "./route-stroke";
import type { GeoPoint } from "@ai-phone/protocol/ai_phone/v1/location_pb";
export type Point = Pick<GeoPoint, "latitude" | "longitude" | "accuracyM">;
export type MapState = {
  point: Point;
  route: Point[];
  editing: boolean;
  interactive: boolean;
  onPoint: (p: Point) => void;
  onRoute: (p: Point[]) => void;
};
function coordinates(p: L.LatLng): Point {
  const wrapped = p.wrap();
  return { latitude: wrapped.lat, longitude: wrapped.lng, accuracyM: 5 };
}
function select(state: MapState, p: L.LatLng) {
  if (!state.interactive) return;
  const point = coordinates(p);
  if (state.editing) state.onRoute([...state.route, point]);
  else state.onPoint(point);
}
function tiles(root: HTMLElement, map: L.Map) {
  L.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", {
    maxZoom: 19,
    referrerPolicy: "strict-origin-when-cross-origin",
    attribution:
      '© <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noreferrer">OpenStreetMap</a>',
  })
    .on("tileerror", () => root.setAttribute("data-tile-error", "true"))
    .on("tileload", () => root.removeAttribute("data-tile-error"))
    .addTo(map);
}
function marker(map: L.Map, get: () => MapState) {
  const p = get().point;
  const icon = L.divIcon({
    className: "location-marker",
    html: "<span></span>",
    iconSize: [26, 26],
    iconAnchor: [13, 13],
  });
  const pin = L.marker([p.latitude, p.longitude], {
    icon,
    draggable: true,
    keyboard: true,
    title: "Location",
  }).addTo(map);
  pin.on("dragend", () => {
    if (get().interactive) get().onPoint(coordinates(pin.getLatLng()));
  });
  return pin;
}
function routeLayers(
  state: MapState,
  line: L.Polyline,
  nodes: L.LayerGroup,
  get: () => MapState,
) {
  line.setLatLngs(state.route.map((p) => [p.latitude, p.longitude]));
  nodes.clearLayers();
  state.route.forEach((p, index) => {
    L.circleMarker([p.latitude, p.longitude], {
      radius: 5,
      color: "#587c73",
      fillOpacity: 1,
    })
      .bindTooltip(String(index + 1))
      .on("click", (event) => {
        L.DomEvent.stopPropagation(event);
        if (get().interactive)
          get().onRoute(get().route.filter((_, i) => i !== index));
      })
      .addTo(nodes);
  });
}
function movePin(map: L.Map, pin: L.Marker, p: Point) {
  const coordinate: L.LatLngTuple = [p.latitude, p.longitude];
  pin.setLatLng(coordinate);
  if (!map.getBounds().contains(coordinate))
    map.panTo(coordinate, {
      animate: !matchMedia("(prefers-reduced-motion:reduce)").matches,
    });
}
function keyboard(event: KeyboardEvent, map: L.Map, get: () => MapState) {
  if (event.key === "Enter" && event.target === map.getContainer()) {
    event.preventDefault();
    select(get(), map.getCenter());
  }
}
export function createLocationMap(root: HTMLElement, get: () => MapState) {
  const p = get().point,
    map = L.map(root, { zoomControl: false, attributionControl: true }).setView(
      [p.latitude, p.longitude],
      14,
    );
  L.control.zoom({ position: "topright" }).addTo(map);
  tiles(root, map);
  const pin = marker(map, get),
    line = L.polyline([], { color: "#587c73", weight: 3 }).addTo(map),
    nodes = L.layerGroup().addTo(map);
  const key = (event: KeyboardEvent) => keyboard(event, map, get);
  let stroke: {id: number; points: L.Point[]; moved: boolean} | null = null;
  let suppressClick = false;
  const repaint = () => routeLayers(get(), line, nodes, get);
  const sync = () => {
    const state = get();
    root.dataset.drawing = String(state.editing && state.interactive);
    if (state.editing && state.interactive) map.dragging.disable();
    else map.dragging.enable();
    if (state.interactive && !state.editing) pin.dragging?.enable();
    else pin.dragging?.disable();
    if ((!state.editing || !state.interactive) && stroke) {stroke=null; repaint();}
  };
  const down = (event: PointerEvent) => {
    if (!get().editing || !get().interactive || event.button !== 0 || stroke) return;
    if (event.target instanceof Element && event.target.closest('.leaflet-control,.leaflet-marker-icon,.leaflet-interactive')) return;
    suppressClick = false;
    stroke = {id:event.pointerId, points:[map.mouseEventToContainerPoint(event)], moved:false};
    root.setPointerCapture(event.pointerId);
    event.preventDefault();
  };
  const move = (event: PointerEvent) => {
    if (!stroke || stroke.id !== event.pointerId) return;
    const next = map.mouseEventToContainerPoint(event);
    if (next.distanceTo(stroke.points[stroke.points.length-1]) < 3) return;
    stroke.moved = true;
    stroke.points.push(next);
    // Keep long strokes bounded without imposing a fixed route length in metres.
    if (stroke.points.length > 2000) stroke.points = simplifyStroke(stroke.points, 600);
    line.setLatLngs([...get().route.map(p=>L.latLng(p.latitude,p.longitude)),...stroke.points.map(p=>map.containerPointToLatLng(p))]);
  };
  const end = (event: PointerEvent) => {
    if (!stroke || stroke.id !== event.pointerId) return;
    const drawn = stroke; stroke = null;
    if (root.hasPointerCapture(event.pointerId)) root.releasePointerCapture(event.pointerId);
    if (event.type === 'pointercancel' || !get().interactive || !get().editing) {suppressClick=true; repaint(); return;}
    if (!drawn.moved) return; // Leaflet click still adds an individual waypoint.
    suppressClick = true;
    const all = [...get().route.map(p=>map.latLngToContainerPoint([p.latitude,p.longitude])),...drawn.points];
    get().onRoute(simplifyStroke(all).map(p=>coordinates(map.containerPointToLatLng(p))));
  };
  map.on("click", (event) => {if(suppressClick){suppressClick=false;return;} select(get(), event.latlng);});
  root.addEventListener("keydown", key);
  root.addEventListener('pointerdown', down);
  root.addEventListener('pointermove', move);
  root.addEventListener('pointerup', end);
  root.addEventListener('pointercancel', end);
  sync();
  const resize = new ResizeObserver(() =>
    map.invalidateSize({ animate: false }),
  );
  resize.observe(root);
  routeLayers(get(), line, nodes, get);
  return {
    point: (value: Point) => movePin(map, pin, value),
    route: () => routeLayers(get(), line, nodes, get),
    interactive: sync,
    destroy: () => {
      resize.disconnect();
      root.removeEventListener("keydown", key);
      root.removeEventListener('pointerdown', down);
      root.removeEventListener('pointermove', move);
      root.removeEventListener('pointerup', end);
      root.removeEventListener('pointercancel', end);
      map.remove();
    },
  };
}
export type LocationMapController = ReturnType<typeof createLocationMap>;
