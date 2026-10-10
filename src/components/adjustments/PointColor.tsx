import { Pipette, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useEditorStore } from '../../store/useEditorStore';
import { POINT_COLOR_LIMIT, POINT_COLOR_RANGES, pointColorSwatch, type PointColor } from '../../utils/pointColor';
import Slider from '../ui/Slider';

interface Props {
  points: PointColor[];
  onChange(points: PointColor[]): void;
  isForMask?: boolean;
  onDragStateChange?(dragging: boolean): void;
}
export default function PointColorPanel({ points, onChange, isForMask = false, onDragStateChange }: Props) {
  const { t } = useTranslation();
  const selectedId = useEditorStore((state) => state.selectedPointColorId);
  const pickerActive = useEditorStore((state) => state.isPointColorPickerActive);
  const pickerMask = useEditorStore((state) => state.pointColorPickerMaskId);
  const maskId = useEditorStore((state) => state.activeMaskContainerId);
  const setEditor = useEditorStore((state) => state.setEditor);
  const selected = points.find((point) => point.id === selectedId) ?? points[0];
  const scope = isForMask ? maskId : null;
  const active = pickerActive && pickerMask === scope;
  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <button
          type="button"
          aria-label={t('pointColor.pick')}
          title={t('pointColor.pick')}
          aria-pressed={active}
          disabled={isForMask && !maskId}
          onClick={() =>
            setEditor({
              isPointColorPickerActive: !active,
              pointColorPickerMaskId: scope,
              isWbPickerActive: false,
              mixerPickerProperty: null,
            })
          }
          className={`p-1.5 rounded-md ${active ? 'bg-accent text-button-text' : 'hover:bg-bg-secondary text-text-secondary'}`}
        >
          <Pipette size={16} />
        </button>
        <div className="flex flex-1 gap-1 flex-wrap">
          {points.map((point, index) => (
            <button
              type="button"
              key={point.id}
              aria-label={t('pointColor.select', { index: index + 1 })}
              aria-pressed={selected?.id === point.id}
              onClick={() => setEditor({ selectedPointColorId: point.id })}
              className={`h-6 w-6 rounded-full border-2 ${selected?.id === point.id ? 'border-text-primary' : 'border-transparent'}`}
              style={{ backgroundColor: pointColorSwatch(point.color) }}
            />
          ))}
        </div>
        <span className="text-xs text-text-secondary tabular-nums">
          {points.length}/{POINT_COLOR_LIMIT}
        </span>
        <button
          type="button"
          aria-label={t('pointColor.delete')}
          disabled={!selected}
          onClick={() => {
            if (selected) onChange(points.filter((point) => point.id !== selected.id));
          }}
          className="p-1.5 text-text-secondary disabled:opacity-30 hover:bg-bg-secondary rounded-md"
        >
          <Trash2 size={16} />
        </button>
      </div>
      {!selected && <p className="text-xs text-text-secondary">{t('pointColor.empty')}</p>}
      {selected &&
        Object.entries(POINT_COLOR_RANGES).map(([key, [min, max, step]]) => (
          <Slider
            key={key}
            data-adjustment-key="pointColor"
            label={t(`pointColor.${key}` as never)}
            min={min}
            max={max}
            step={step}
            value={selected[key as keyof typeof POINT_COLOR_RANGES]}
            defaultValue={
              key.endsWith('Shift')
                ? 0
                : key === 'hueRange'
                  ? 40
                  : key === 'chromaRange'
                    ? 0.1
                    : key === 'lightnessRange'
                      ? 0.25
                      : 50
            }
            onDragStateChange={onDragStateChange}
            onChange={(event) =>
              onChange(
                points.map((point) =>
                  point.id === selected.id ? { ...point, [key]: Number(event.target.value) } : point,
                ),
              )
            }
          />
        ))}
      {selected && <p className="text-xs text-text-secondary">{t('pointColor.bounded')}</p>}
    </div>
  );
}
