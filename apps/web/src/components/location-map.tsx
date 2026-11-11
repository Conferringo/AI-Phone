"use client";
import "leaflet/dist/leaflet.css";
import { useEffect, useRef } from "react";
import type {
  MapState,
  LocationMapController,
} from "@/lib/location-map-controller";
export function LocationMap(props: MapState & { errorLabel: string }) {
  const root = useRef<HTMLDivElement>(null),
    controller = useRef<LocationMapController | null>(null),
    latest = useRef(props);
  latest.current = props;
  useEffect(() => {
    let disposed = false;
    void import("@/lib/location-map-controller")
      .then(({ createLocationMap }) => {
        if (!disposed && root.current) {
          controller.current = createLocationMap(
            root.current,
            () => latest.current,
          );
          controller.current.interactive();
        }
      })
      .catch(() => root.current?.setAttribute("data-tile-error", "true"));
    return () => {
      disposed = true;
      controller.current?.destroy();
      controller.current = null;
    };
  }, []);
  useEffect(
    () => controller.current?.point(props.point),
    [props.point.latitude, props.point.longitude],
  );
  useEffect(
    () => controller.current?.interactive(),
    [props.interactive, props.editing],
  );
  useEffect(() => controller.current?.route(), [props.route]);
  return (
    <div className="location-map-shell">
      <div ref={root} className="location-map" role="region" aria-label="Map" />
      <span className="location-map-error">{props.errorLabel}</span>
    </div>
  );
}
