import { test, expect } from "bun:test";
import { create } from "@bufbuild/protobuf";
import {
  LocationStateSchema,
  GeoPointSchema,
} from "../packages/protocol/src/gen/ai_phone/v1/location_pb";
import {
  draftFrom,
  reducer,
  requestFrom,
  type View,
} from "../apps/web/src/components/location-state";
const state = () =>
  create(LocationStateSchema, {
    revision: 1n,
    enabled: true,
    point: create(GeoPointSchema, {
      latitude: 31,
      longitude: 121,
      accuracyM: 5,
    }),
    speedKph: 5,
    updatedAtMs: 10n,
  });
function view(): View {
  const snapshot = state();
  return {
    snapshot,
    draft: draftFrom(snapshot),
    pending: false,
    error: "",
    loadError: "",
  };
}
test("polling retains coordinate drafts and errors, rejects stale snapshots and avoids repainting unchanged routes", () => {
  let value = view();
  value = reducer(value, {
    type: "edit",
    value: { coordinates: { latitude: "-", longitude: "121" } },
  });
  value = reducer(value, { type: "error", value: "请输入有效的经纬度" });
  const same = reducer(value, { type: "read", value: state() });
  expect(same).toBe(value);
  const next = reducer(value, {
    type: "read",
    value: { ...state(), revision: 2n },
  });
  expect(next.draft.coordinates.latitude).toBe("-");
  expect(next.error).toBe(value.error);
  expect(reducer(next, { type: "read", value: state() })).toBe(next);
  expect(() => requestFrom(next.snapshot!, next.draft, {})).toThrow("经纬度");
});
test("in-flight poll cannot roll back a save; authoritative save resets drafts exactly once", () => {
  const current = view(),
    pending = reducer(current, { type: "begin" });
  expect(
    reducer(pending, { type: "read", value: { ...state(), revision: 2n } }),
  ).toBe(pending);
  const edited = reducer(pending, { type: "edit", value: { speed: "12" } });
  const saved = reducer(edited, {
    type: "saved",
    value: { ...state(), revision: 2n, speedKph: 12 },
  });
  expect(saved.draft.dirty).toBe(false);
  expect(saved.draft.speed).toBe("12");
  expect(
    requestFrom(saved.snapshot!, saved.draft, { moving: false })
      .expectedRevision,
  ).toBe(2n);
});
test("turning off does not parse unfinished input or request host lookup; enabling explicitly opts into lookup", () => {
  const value = view();
  value.draft.coordinates.latitude = "-";
  const off = requestFrom(value.snapshot!, value.draft, { enabled: false });
  expect(off.enabled).toBe(false);
  expect(off.useHost).toBeUndefined();
  const enabled = requestFrom(value.snapshot!, value.draft, {
    enabled: true,
    useHost: true,
  });
  expect(enabled.useHost).toBe(true);
});

test("retrying a network-derived fix keeps its declared accuracy until coordinates actually change", () => {
  const snapshot = {
    ...state(),
    point: create(GeoPointSchema, {
      latitude: 31,
      longitude: 121,
      accuracyM: 50000,
    }),
  };
  const draft = draftFrom(snapshot);
  expect(requestFrom(snapshot, draft, {}).point?.accuracyM).toBe(50000);
  draft.coordinates.latitude = "31.001";
  expect(requestFrom(snapshot, draft, {}).point?.accuracyM).toBe(5);
});
