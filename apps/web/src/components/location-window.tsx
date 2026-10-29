"use client";
import { useEffect, useState } from "react";
import { create } from "@bufbuild/protobuf";
import { GeoPointSchema } from "@ai-phone/protocol/ai_phone/v1/location_pb";
import { LocateFixed, Pause, Play, Route, Undo2 } from "lucide-react";
import { FloatingWindow } from "./floating-window";
import { Button } from "./ui/button";
import { SelectField } from "./ui/select";
import { useI18n } from "./locale-provider";
import { LocationMap } from "./location-map";
import {
  useLocationController,
  type LocationController,
  type LocationState,
} from "./location-state";
import "./location-window.css";
function LocationToggle({ control }: { control: LocationController }) {
  const { t } = useI18n(),
    { view, save } = control;
  return (
    <label className="location-toggle">
      <input
        type="checkbox"
        checked={view.snapshot?.enabled ?? false}
        disabled={!view.snapshot || view.pending}
        aria-label={t("启用模拟位置")}
        onChange={(e) =>
          void save({
            enabled: e.target.checked,
            useHost: e.target.checked && !view.draft.point,
          })
        }
      />
      <span aria-hidden="true" />
    </label>
  );
}
function Coordinates({
  control,
  state,
}: {
  control: LocationController;
  state: LocationState;
}) {
  const { t } = useI18n(),
    { view, edit, save } = control,
    { draft, pending } = view;
  return (
    <div className="location-coordinates">
      {(["latitude", "longitude"] as const).map((key) => (
        <label key={key}>
          {t(key === "latitude" ? "纬度" : "经度")}
          <input
            type="text"
            inputMode="decimal"
            value={draft.coordinates[key]}
            disabled={pending || state.moving}
            onChange={(e) =>
              edit({
                coordinates: { ...draft.coordinates, [key]: e.target.value },
              })
            }
          />
        </label>
      ))}
      <Button
        disabled={!draft.dirty || pending || state.moving}
        onClick={() => void save()}
      >
        {t("应用")}
      </Button>
    </div>
  );
}
function MotionControls({
  control,
  state,
}: {
  control: LocationController;
  state: LocationState;
}) {
  const { t } = useI18n(),
    { view, edit, save } = control,
    { draft, pending } = view;
  return (
    <div className="location-motion">
      <label>
        {t("速度")}
        <div className="location-speed">
          <input
            aria-label={t("速度")}
            type="number"
            min="0.5"
            max="500"
            step="0.5"
            value={draft.speed}
            disabled={pending || state.moving}
            onChange={(e) => edit({ speed: e.target.value })}
          />
          <span>km/h</span>
        </div>
      </label>
      <label>
        {t("播放")}
        <SelectField
          aria-label={t("路线播放")}
          value={draft.repeat ? "repeat" : "once"}
          options={[
            { value: "once", label: t("单次") },
            { value: "repeat", label: t("循环") },
          ]}
          disabled={pending || state.moving}
          onValueChange={(v) => edit({ repeat: v === "repeat" })}
        />
      </label>
      <Button
        disabled={pending || draft.route.length < 2}
        aria-label={state.moving ? t("暂停") : t("开始移动")}
        onClick={() => void save({ moving: !state.moving })}
      >
        {state.moving ? <Pause size={16} /> : <Play size={16} />}
      </Button>
    </div>
  );
}
function receiptStatus(device: LocationState["devices"][number]) {
  if (device.status === "disabled") return "已关闭";
  if (device.status !== "applied") return "未同步";
  return device.kind === "desktop" ? "浏览器已同步" : "已同步";
}
function DeviceReceipts({ devices }: { devices: LocationState["devices"] }) {
  const { t } = useI18n();
  return (
    <div className="location-devices">
      {devices.map((d) => (
        <span
          key={d.deviceId}
          data-failed={d.status === "failed"}
          title={d.error}
        >
          {t(d.kind === "phone" ? "手机" : "电脑")} · {t(receiptStatus(d))}
          {d.error && <span role="status">{t(d.error)}</span>}
        </span>
      ))}
    </div>
  );
}
function LocationActions({
  control,
  state,
  editing,
  setEditing,
}: {
  control: LocationController;
  state: LocationState;
  editing: boolean;
  setEditing: (v: boolean | ((v: boolean) => boolean)) => void;
}) {
  const { t } = useI18n(),
    { view, edit, save } = control,
    { draft, pending } = view;
  return (
    <div className="location-actions">
      <Button
        variant="ghost"
        disabled={pending || state.moving}
        onClick={() => void save({ useHost: true, moving: false, route: [] })}
      >
        <LocateFixed size={16} />
        {t("跟随宿主位置")}
      </Button>
      <Button
        variant={editing ? "secondary" : "ghost"}
        disabled={pending || state.moving}
        aria-pressed={editing}
        onClick={() => setEditing((v) => !v)}
      >
        <Route size={16} />
        {t("路线")}
      </Button>
      {editing && (
        <Button
          variant="ghost"
          size="icon"
          aria-label={t("撤销路线点")}
          disabled={!draft.route.length || pending}
          onClick={() => edit({ route: draft.route.slice(0, -1) })}
        >
          <Undo2 size={16} />
        </Button>
      )}
    </div>
  );
}
function LocationControls({
  control,
  state,
}: {
  control: LocationController;
  state: LocationState;
}) {
  const { t } = useI18n(),
    { view, edit, choose, save } = control,
    { draft, pending } = view,
    [editing, setEditing] = useState(false);
  return (
    <>
      <LocationActions
        control={control}
        state={state}
        editing={editing}
        setEditing={setEditing}
      />
      {state.source === "network" && !draft.dirty && (
        <span className="location-source">{t("宿主网络近似位置")}</span>
      )}
      {draft.point && (
        <>
          <LocationMap
            point={draft.point}
            route={draft.route}
            editing={editing}
            interactive={!pending && !state.moving}
            onPoint={(p) => choose(create(GeoPointSchema, p))}
            onRoute={(r) =>
              edit({ route: r.map((p) => create(GeoPointSchema, p)) })
            }
            errorLabel={t("地图暂不可用，可输入经纬度")}
          />
          <Coordinates control={control} state={state} />
        </>
      )}
      {(editing || draft.route.length > 0) && (
        <MotionControls control={control} state={state} />
      )}
    </>
  );
}
function LocationContent({ control }: { control: LocationController }) {
  const { t } = useI18n(),
    { snapshot } = control.view;
  if (!snapshot) return <div role="status">{t("加载中")}</div>;
  if (snapshot.enabled)
    return <LocationControls control={control} state={snapshot} />;
  return (
    <div className="location-off">
      <LocateFixed size={28} />
      <Button
        disabled={control.view.pending}
        onClick={() =>
          void control.save({
            enabled: true,
            useHost: !control.view.draft.point,
          })
        }
      >
        {t("开启")}
      </Button>
    </div>
  );
}
export function LocationWindow({
  open,
  onClose,
  onEnabled,
}: {
  open: boolean;
  onClose: () => void;
  onEnabled: (v: boolean) => void;
}) {
  const { t } = useI18n(),
    control = useLocationController(open),
    { view } = control;
  useEffect(() => {
    if (view.snapshot) onEnabled(view.snapshot.enabled);
  }, [view.snapshot?.enabled, onEnabled]);
  const error = view.error || view.loadError || view.snapshot?.error;
  return (
    <FloatingWindow
      fitContent
      id="location"
      title={t("模拟位置")}
      open={open}
      onClose={onClose}
      width={720}
      height={750}
      actions={<LocationToggle control={control} />}
    >
      <div className="location-panel" aria-busy={view.pending}>
        {error && (
          <p role="alert" className="workbench-error">
            {t(error)}
          </p>
        )}
        <LocationContent control={control} />
        <DeviceReceipts devices={view.snapshot?.devices ?? []} />
      </div>
    </FloatingWindow>
  );
}
