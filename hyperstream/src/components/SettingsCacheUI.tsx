import { useState, useEffect } from 'react';
import type { AppSettings } from '../types';
import { useSettingsCache } from '../hooks/useSettingsCache';

/**
 * SettingsCacheStatus - Displays cache freshness and metrics
 * 
 * Shows:
 * - Cache freshness indicator
 * - Age of cached data
 * - Generation counter for external change detection
 * - Manual invalidation option
 */
export function SettingsCacheStatus() {
  const { cacheStats, isCacheFresh, cacheAgeSeconds, invalidateCache, isLoading } =
    useSettingsCache();

  if (!cacheStats) {
    return <div className="text-gray-400">Loading cache stats...</div>;
  }

  const formattedAge = cacheAgeSeconds.toFixed(1);
  const statusIcon = isCacheFresh ? '✓' : '⟳';
  const statusColor = isCacheFresh ? 'text-green-500' : 'text-yellow-500';

  return (
    <div className="p-3 bg-gray-800/30 rounded border border-gray-700/50 text-sm">
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-2">
          <span className={`text-lg ${statusColor}`}>{statusIcon}</span>
          <div>
            <div className="text-gray-300">
              Cache: <span className={statusColor}>{isCacheFresh ? 'Fresh' : 'Stale'}</span>
            </div>
            <div className="text-gray-500 text-xs">
              Age: {formattedAge}s | Gen: {cacheStats.generation}
            </div>
          </div>
        </div>
        <button
          onClick={invalidateCache}
          disabled={isLoading}
          className="px-3 py-1 bg-gray-700 hover:bg-gray-600 rounded text-xs disabled:opacity-50"
        >
          {isLoading ? 'Reloading...' : 'Reload'}
        </button>
      </div>
    </div>
  );
}

/**
 * SettingsValidationFeedback - Inline validation feedback for settings forms
 * 
 * Shows:
 * - Real-time validation errors as user edits
 * - Critical vs warning severity indicators
 * - Per-field error messages
 */
export function SettingsValidationFeedback({
  settings,
}: {
  settings: AppSettings;
}) {
  const { validateDraft, lastValidation } = useSettingsCache();
  const [isValidating, setIsValidating] = useState(false);

  useEffect(() => {
    const validate = async () => {
      setIsValidating(true);
      await validateDraft(settings);
      setIsValidating(false);
    };

    // Debounce validation on settings change
    const timer = setTimeout(validate, 500);
    return () => clearTimeout(timer);
  }, [settings, validateDraft]);

  if (!lastValidation) {
    return null;
  }

  const criticalErrors = lastValidation.errors.filter(e => e.is_critical);
  const warnings = lastValidation.errors.filter(e => !e.is_critical);

  return (
    <div className="space-y-2">
      {isValidating && (
        <div className="text-xs text-gray-400">Validating settings...</div>
      )}

      {criticalErrors.length > 0 && (
        <div className="p-2 bg-red-500/10 border border-red-500/30 rounded">
          <div className="text-red-400 text-xs font-semibold mb-1">Errors:</div>
          {criticalErrors.map((error, i) => (
            <div key={i} className="text-red-300 text-xs">
              • {error.field}: {error.message}
            </div>
          ))}
        </div>
      )}

      {warnings.length > 0 && (
        <div className="p-2 bg-yellow-500/10 border border-yellow-500/30 rounded">
          <div className="text-yellow-400 text-xs font-semibold mb-1">Warnings:</div>
          {warnings.map((warning, i) => (
            <div key={i} className="text-yellow-300 text-xs">
              • {warning.field}: {warning.message}
            </div>
          ))}
        </div>
      )}

      {lastValidation.valid && !isValidating && (
        <div className="text-green-400 text-xs">✓ All settings valid</div>
      )}

      {lastValidation.warnings.length > 0 && (
        <div className="text-gray-400 text-xs">
          {lastValidation.warnings.map((w, i) => (
            <div key={i}>• {w}</div>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * SegmentsSettingField - Example of a validated settings field
 * 
 * Shows how to integrate cache validation with form fields
 */
export function SegmentsSettingField({
  value,
  onChange,
}: {
  value: number;
  onChange: (value: number) => void;
}) {
  const [error, setError] = useState<string | null>(null);

  const handleChange = (newValue: number) => {
    onChange(newValue);

    // Show immediate bounds checking
    if (newValue < 1) {
      setError('Minimum: 1 segment');
    } else if (newValue > 64) {
      setError('Maximum: 64 segments');
    } else {
      setError(null);
    }
  };

  return (
    <div className="space-y-2">
      <label className="block text-sm text-gray-300">
        Download Segments
      </label>
      <input
        type="number"
        value={value}
        onChange={e => handleChange(parseInt(e.target.value) || 1)}
        min={1}
        max={64}
        className={`w-full px-3 py-2 bg-gray-700 rounded border ${
          error
            ? 'border-red-500/50 bg-red-500/5'
            : 'border-gray-600 hover:border-gray-500'
        } text-white`}
      />
      {error && (
        <div className="text-red-400 text-xs">
          ⚠ {error}
        </div>
      )}
      <div className="text-gray-500 text-xs">
        Controls parallel download segments (1-64). More segments = faster but more CPU.
      </div>
    </div>
  );
}

/**
 * SaveSettingsButton - Settings save button with validation feedback
 * 
 * Shows:
 * - Pre-flight validation
 * - Save operation feedback
 * - Cache invalidation after save
 */
export function SaveSettingsButton({
  settings,
  onSave,
  disabled,
}: {
  settings: AppSettings;
  onSave: (success: boolean) => void;
  disabled?: boolean;
}) {
  const { saveDraft, lastValidation } = useSettingsCache();
  const [isSaving, setIsSaving] = useState(false);
  const [saveStatus, setSaveStatus] = useState<'idle' | 'success' | 'error'>('idle');

  const handleSave = async () => {
    if (disabled || isSaving) return;

    setIsSaving(true);
    setSaveStatus('idle');

    try {
      const result = await saveDraft(settings);

      if (result.success) {
        setSaveStatus('success');
        onSave(true);
        setTimeout(() => setSaveStatus('idle'), 3000);
      } else {
        setSaveStatus('error');
        onSave(false);
      }
    } catch (_err) {
      setSaveStatus('error');
      onSave(false);
    } finally {
      setIsSaving(false);
    }
  };

  const isValid = lastValidation?.valid ?? true;
  const hasErrors = (lastValidation?.errors ?? []).some(e => e.is_critical);

  return (
    <div className="space-y-2">
      <button
        onClick={handleSave}
        disabled={disabled || isSaving || hasErrors || !isValid}
        className={`w-full px-4 py-2 rounded font-semibold transition ${
          disabled || isSaving || hasErrors || !isValid
            ? 'bg-gray-600 text-gray-400 cursor-not-allowed opacity-50'
            : saveStatus === 'success'
              ? 'bg-green-600 hover:bg-green-700 text-white'
              : saveStatus === 'error'
                ? 'bg-red-600 hover:bg-red-700 text-white'
                : 'bg-blue-600 hover:bg-blue-700 text-white'
        }`}
      >
        {isSaving ? 'Saving...' : saveStatus === 'success' ? '✓ Saved' : 'Save Settings'}
      </button>
      {hasErrors && (
        <div className="text-red-400 text-xs text-center">
          Fix validation errors before saving
        </div>
      )}
    </div>
  );
}
