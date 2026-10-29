"use client";
import { useEffect, useReducer, useRef } from "react";
import { create, type MessageShape } from "@bufbuild/protobuf";
import {
  GeoPointSchema,
  LocationRequestSchema,
  LocationUpdateSchema,
  LocationStateSchema,
} from "@ai-phone/protocol/ai_phone/v1/location_pb";
import { settingsRpc } from "@/lib/settings-client";
import { clientId } from "@/lib/client-id";
export type LocationState = MessageShape<typeof LocationStateSchema>;
export type Point = MessageShape<typeof GeoPointSchema>;
export type Draft = {
  point: Point | null;
  coordinates: { latitude: string; longitude: string };
  route: Point[];
  speed: string;
  repeat: boolean;
  dirty: boolean;
};
export type LocationPatch = {
  enabled?: boolean;
  useHost?: boolean;
  moving?: boolean;
  route?: Point[];
};
export type View = {
  snapshot: LocationState | null;
  draft: Draft;
  pending: boolean;
  error: string;
  loadError: string;
};
type Action =
  | { type: "read" | "saved"; value: LocationState }
  | { type: "point"; value: Point }
  | { type: "edit"; value: Partial<Draft> }
  | { type: "begin" | "finish" }
  | { type: "error" | "load-error"; value: string };
export function draftFrom(state: LocationState): Draft {
  const point =
    state.point ??
    (state.enabled ? create(GeoPointSchema, { accuracyM: 5 }) : null);
  return {
    point,
    coordinates: {
      latitude: point ? String(point.latitude) : "",
      longitude: point ? String(point.longitude) : "",
    },
    route: state.route,
    speed: String(state.speedKph || 5),
    repeat: state.repeat,
    dirty: false,
  };
}
function newer(previous: LocationState | null, next: LocationState) {
  return (
    !previous ||
    next.revision > previous.revision ||
    (next.revision === previous.revision &&
      next.updatedAtMs > previous.updatedAtMs)
  );
}
function receivedDraft(
  view: View,
  value: LocationState,
  reset: boolean,
): Draft {
  if (reset || !view.snapshot) return draftFrom(value);
  if (value.moving && !view.draft.dirty) {
    const next = draftFrom(value);
    return { ...view.draft, point: next.point, coordinates: next.coordinates };
  }
  return view.draft;
}
function accept(view: View, value: LocationState, reset: boolean): View {
  if (view.pending && !reset) return view;
  if (!newer(view.snapshot, value))
    return view.loadError ? { ...view, loadError: "" } : view;
  return {
    ...view,
    snapshot: value,
    draft: receivedDraft(view, value, reset),
    loadError: "",
    error: reset ? "" : view.error,
  };
}
export function reducer(view: View, action: Action): View {
  switch (action.type) {
    case "read":
      return accept(view, action.value, false);
    case "saved":
      return accept(view, action.value, true);
    case "point":
      return {
        ...view,
        draft: {
          ...view.draft,
          point: action.value,
          coordinates: {
            latitude: String(action.value.latitude),
            longitude: String(action.value.longitude),
          },
          dirty: true,
        },
      };
    case "edit":
      return {
        ...view,
        draft: { ...view.draft, ...action.value, dirty: true },
      };
    case "begin":
      return { ...view, pending: true, error: "" };
    case "finish":
      return { ...view, pending: false };
    case "error":
      return { ...view, error: action.value };
    case "load-error":
      return { ...view, loadError: action.value };
  }
}
function coordinateAccuracy(point: Point, latitude: number, longitude: number) {
  return point.latitude === latitude && point.longitude === longitude
    ? point.accuracyM
    : 5;
}
function selectedPoint(draft: Draft): Point | undefined {
  if (!draft.point) return undefined;
  const latitude = Number(draft.coordinates.latitude),
    longitude = Number(draft.coordinates.longitude);
  if (
    !draft.coordinates.latitude.trim() ||
    !draft.coordinates.longitude.trim() ||
    !Number.isFinite(latitude) ||
    !Number.isFinite(longitude) ||
    Math.abs(latitude) > 90 ||
    Math.abs(longitude) > 180
  )
    throw Error("请输入有效的经纬度");
  return create(GeoPointSchema, {
    latitude,
    longitude,
    accuracyM: coordinateAccuracy(draft.point, latitude, longitude),
  });
}
export function requestFrom(
  snapshot: LocationState,
  draft: Draft,
  patch: LocationPatch,
) {
  const enabled = patch.enabled ?? snapshot.enabled;
  const point =
    enabled && !patch.useHost ? selectedPoint(draft) : snapshot.point;
  if (enabled && !patch.useHost && !point) throw Error("请选择位置");
  return {
    expectedRevision: snapshot.revision,
    requestId: clientId(),
    enabled,
    point: point ?? undefined,
    route: draft.route,
    speedKph: Number(draft.speed),
    repeat: draft.repeat,
    moving: false,
    ...patch,
  };
}
function poll(
  open: boolean,
  onState: (s: LocationState) => void,
  onError: (e: string) => void,
) {
  let cancelled = false,
    timer: ReturnType<typeof setTimeout>;
  const load = async () => {
    try {
      const value = await settingsRpc(
        "state",
        LocationRequestSchema,
        {},
        LocationStateSchema,
        "location",
      );
      if (!cancelled) onState(value);
    } catch (e) {
      if (!cancelled) onError(e instanceof Error ? e.message : "读取失败");
    }
    if (!cancelled && open) timer = setTimeout(load, 1500);
  };
  void load();
  return () => {
    cancelled = true;
    clearTimeout(timer);
  };
}
export function useLocationController(open: boolean) {
  const [view, dispatch] = useReducer(reducer, {
    snapshot: null,
    draft: draftFrom(create(LocationStateSchema)),
    pending: false,
    error: "",
    loadError: "",
  });
  const pending = useRef(false);
  useEffect(
    () =>
      poll(
        open,
        (s) => dispatch({ type: "read", value: s }),
        (e) => dispatch({ type: "load-error", value: e }),
      ),
    [open],
  );
  async function save(patch: LocationPatch = {}) {
    if (!view.snapshot || pending.current) return;
    let request;
    try {
      request = requestFrom(view.snapshot, view.draft, patch);
    } catch (e) {
      dispatch({
        type: "error",
        value: e instanceof Error ? e.message : "更新失败",
      });
      return;
    }
    pending.current = true;
    dispatch({ type: "begin" });
    try {
      dispatch({
        type: "saved",
        value: await settingsRpc(
          "update",
          LocationUpdateSchema,
          request,
          LocationStateSchema,
          "location",
        ),
      });
    } catch (e) {
      dispatch({
        type: "error",
        value: e instanceof Error ? e.message : "更新失败",
      });
    } finally {
      pending.current = false;
      dispatch({ type: "finish" });
    }
  }
  return {
    view,
    save,
    edit: (value: Partial<Draft>) => dispatch({ type: "edit", value }),
    choose: (value: Point) => dispatch({ type: "point", value }),
  };
}
export type LocationController = ReturnType<typeof useLocationController>;
