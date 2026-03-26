/**
 * Example: Using Settings Cache System in a Real Settings Page
 * 
 * This demonstrates how to integrate the Settings Cache System
 * into an existing settings page component.
 */

import { useState, useEffect } from 'react';
import type { AppSettings } from '../types';
import { useSettingsCache } from '../hooks/useSettingsCache';
import * as settingsCache from '../api/settingsCache';
import {
  SettingsCacheStatus,
  SettingsValidationFeedback,
  SaveSettingsButton,
  SegmentsSettingField,
} from './SettingsCacheUI';

/**
 * Example SettingsPage with integrated Settings Cache System
 * 
 * This component shows:
 * 1. How to use the useSettingsCache hook
 * 2. How to display cache status
 * 3. How to show real-time validation feedback
 * 4. How to save with validation
 */
export function ExampleSettingsPage() {
  // 1. Initialize the cache hook
  const {
    cacheStats,
    lastValidation,
    validateDraft,
    isLoading,
  } = useSettingsCache();

  // 2. Local state for settings form
  const [settings, setSettings] = useState<AppSettings>(() => {
    // Load initial settings (from API or defaults)
    return {
      segments: 4,
      download_dir: '/downloads',
      max_connections_per_host: 6,
      // ... other settings
    } as AppSettings;
  });

  // 3. Auto-validate when settings change
  useEffect(() => {
    const timer = setTimeout(() => {
      validateDraft(settings);
    }, 500); // Debounce to avoid excessive validation

    return () => clearTimeout(timer);
  }, [settings, validateDraft]);

  // 5. Handle field changes
  const handleFieldChange = (fieldName: keyof AppSettings, value: AppSettings[keyof AppSettings]) => {
    setSettings((prev) => ({
      ...prev,
      [fieldName]: value,
    }));
  };

  return (
    <div className="space-y-6 p-6">
      {/* Header with cache status */}
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Settings</h1>
        <SettingsCacheStatus />
      </div>

      {/* Main settings form */}
      <div className="space-y-4 bg-gray-800 rounded p-6">
        {/* Form Fields */}
        <SegmentsSettingField
          value={settings.segments}
          onChange={(val: number) => handleFieldChange('segments', val)}
        />

        {/* Add more field components here... */}

        {/* Validation Feedback */}
        <SettingsValidationFeedback
          settings={settings}
        />
      </div>

      {/* Save Button with Validation */}
      <SaveSettingsButton
        settings={settings}
        onSave={(success: boolean) => {
          if (success) {
            // Show success toast
            console.log('✅ Settings saved');
          } else {
            // Show error toast
            console.log('❌ Settings failed to save');
          }
        }}
        disabled={isLoading}
      />

      {/* Debug Info (Optional - remove in production) */}
      {process.env.NODE_ENV === 'development' && (
        <div className="text-xs text-gray-500 space-y-1">
          <p>Cache Fresh: {cacheStats?.is_fresh ? '✓' : '✗'}</p>
          <p>Cache Age: {cacheStats?.age_secs ?? 'unknown'}s</p>
          <p>Generation: {cacheStats?.generation ?? 'unknown'}</p>
          <p>Errors: {lastValidation?.errors.length ?? 0}</p>
          <p>Warnings: {lastValidation?.warnings.length ?? 0}</p>
        </div>
      )}
    </div>
  );
}

/**
 * Alternative: Using just the API wrapper (without the hook)
 * 
 * For simpler use cases where you don't need auto-polling
 */
export function SimpleSettingsPage() {
  const [settings, setSettings] = useState<AppSettings>({} as AppSettings);

  const handleSave = async () => {
    try {
      // Validate before saving
      const report = await settingsCache.validateSettings(settings);

      if (!report.valid) {
        console.error('Validation failed:', report.errors);
        return;
      }

      // Save with validation report
      const result = await settingsCache.saveSettingsWithValidation(settings);

      if (result.success) {
        console.log('Saved successfully');
      } else {
        console.error('Save failed during validation');
      }
    } catch (err) {
      console.error('Save error:', err);
    }
  };

  return (
    <div className="space-y-4">
      <input
        value={settings.segments}
        onChange={(e) => {
          const newVal = parseInt(e.target.value);
          setSettings((prev) => ({
            ...prev,
            segments: newVal,
          }));
        }}
      />
      <button onClick={handleSave}>Save</button>
    </div>
  );
}

/**
 * Integration Steps for Your Project:
 * 
 * 1. In your existing SettingsPage component:
 *    - Import { useSettingsCache } from '@/hooks/useSettingsCache'
 *    - Add: const { ... } = useSettingsCache()
 *    - Replace your save handler with saveDraft()
 * 
 * 2. Add SettingsCacheStatus to your header:
 *    - Import { SettingsCacheStatus } from '@/components/SettingsCacheUI'
 *    - Add <SettingsCacheStatus /> near your page title
 * 
 * 3. Wrap your form with validation feedback:
 *    - Import { SettingsValidationFeedback } from '@/components/SettingsCacheUI'
 *    - Add <SettingsValidationFeedback settings={settings} ... />
 * 
 * 4. Replace your save button:
 *    - Import { SaveSettingsButton } from '@/components/SettingsCacheUI'
 *    - Replace your <button>Save</button> with:
 *      <SaveSettingsButton settings={settings} onSave={...} />
 * 
 * That's it! Your settings page now has:
 * - Real-time validation feedback
 * - Cache metrics display
 * - Pre-flight validation on save
 * - Automatic cache invalidation
 */
