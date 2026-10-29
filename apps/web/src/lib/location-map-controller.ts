import * as L from "leaflet";
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
  map.on("click", (event) => select(get(), event.latlng));
  root.addEventListener("keydown", key);
  const resize = new ResizeObserver(() =>
    map.invalidateSize({ animate: false }),
  );
  resize.observe(root);
  routeLayers(get(), line, nodes, get);
  return {
    point: (value: Point) => movePin(map, pin, value),
    route: () => routeLayers(get(), line, nodes, get),
    interactive: (enabled: boolean) => {
      if (enabled) pin.dragging?.enable();
      else pin.dragging?.disable();
    },
    destroy: () => {
      resize.disconnect();
      root.removeEventListener("keydown", key);
      map.remove();
    },
  };
}
export type LocationMapController = ReturnType<typeof createLocationMap>;
