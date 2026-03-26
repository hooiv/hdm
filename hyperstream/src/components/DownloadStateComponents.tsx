import { useState, useEffect } from 'react';
import { useDownloadStateManagement, useResumeValidation, getStatusLabel, getStatusColor } from '../hooks/useDownloadStateManagement';
import { AlertCircle, CheckCircle, AlertTriangle, Loader } from 'lucide-react';

interface ResumeSafetyWarningProps {
  downloadId: string;
  onResume: () => Promise<void>;
  onCancel: () => void;
}

/**
 * Component that validates and warns about resume safety
 * Shows different UI based on validation level
 */
export function ResumeSafetyWarning({
  downloadId,
  onResume,
  onCancel,
}: ResumeSafetyWarningProps) {
  const { report, loading } = useResumeValidation(downloadId);
  const [isResuming, setIsResuming] = useState(false);

  const handleResume = async () => {
    setIsResuming(true);
    try {
      await onResume();
    } finally {
      setIsResuming(false);
    }
  };

  if (loading) {
    return (
      <div className="p-4 bg-gray-900/50 border border-gray-700 rounded-lg">
        <div className="flex items-center gap-2">
          <Loader className="w-4 h-4 animate-spin text-cyan-400" />
          <span>Validating download safety...</span>
        </div>
      </div>
    );
  }

  if (!report) {
    return null;
  }

  const { level, cannot_resume, recommendation } = report;

  if (cannot_resume) {
    return (
      <div className="p-4 bg-red-500/10 border border-red-500/30 rounded-lg">
        <div className="flex items-start gap-3">
          <AlertCircle className="w-5 h-5 text-red-400 flex-shrink-0 mt-0.5" />
          <div className="flex-1">
            <p className="font-semibold text-red-300">Cannot Resume Download</p>
            <p className="text-sm text-red-200 mt-1">{recommendation}</p>
            <div className="mt-3 space-y-1">
              {report.checks_failed.map((check: string, i: number) => (
                <p key={i} className="text-xs text-red-300">
                  ✗ {check}
                </p>
              ))}
            </div>
            {report.should_restart_from_scratch && (
              <p className="text-xs text-yellow-300 mt-2 font-semibold">
                💡 Recommendation: Start this download from scratch
              </p>
            )}
            <div className="mt-4 flex gap-2">
              <button
                onClick={onCancel}
                className="px-4 py-2 text-sm bg-gray-700 hover:bg-gray-600 rounded text-gray-200 transition"
              >
                Close
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  if (level === 'warning') {
    return (
      <div className="p-4 bg-yellow-500/10 border border-yellow-500/30 rounded-lg">
        <div className="flex items-start gap-3">
          <AlertTriangle className="w-5 h-5 text-yellow-400 flex-shrink-0 mt-0.5" />
          <div className="flex-1">
            <p className="font-semibold text-yellow-300">Resume with Caution</p>
            <p className="text-sm text-yellow-200 mt-1">{recommendation}</p>
            <div className="mt-3 space-y-1">
              {report.checks_warning.map((check: string, i: number) => (
                <p key={i} className="text-xs text-yellow-300/80">
                  ⚠ {check}
                </p>
              ))}
            </div>
            <div className="mt-4 flex gap-2">
              <button
                onClick={handleResume}
                disabled={isResuming}
                className="px-4 py-2 text-sm bg-yellow-600 hover:bg-yellow-500 disabled:opacity-50 rounded text-white transition font-medium"
              >
                {isResuming ? 'Resuming...' : 'Resume Anyway'}
              </button>
              <button
                onClick={onCancel}
                className="px-4 py-2 text-sm bg-gray-700 hover:bg-gray-600 rounded text-gray-200 transition"
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  if (level === 'caution') {
    return (
      <div className="p-4 bg-blue-500/10 border border-blue-500/30 rounded-lg">
        <div className="flex items-start gap-3">
          <AlertCircle className="w-5 h-5 text-blue-400 flex-shrink-0 mt-0.5" />
          <div className="flex-1">
            <p className="font-semibold text-blue-300">Resume Recommended</p>
            <p className="text-sm text-blue-200 mt-1">{recommendation}</p>
            {report.checks_warning.length > 0 && (
              <div className="mt-3 space-y-1">
                {report.checks_warning.map((check: string, i: number) => (
                  <p key={i} className="text-xs text-blue-300/80">
                    ℹ {check}
                  </p>
                ))}
              </div>
            )}
            <div className="mt-4 flex gap-2">
              <button
                onClick={handleResume}
                disabled={isResuming}
                className="px-4 py-2 text-sm bg-cyan-600 hover:bg-cyan-500 disabled:opacity-50 rounded text-white transition font-medium"
              >
                {isResuming ? 'Resuming...' : 'Resume Download'}
              </button>
              <button
                onClick={onCancel}
                className="px-4 py-2 text-sm bg-gray-700 hover:bg-gray-600 rounded text-gray-200 transition"
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // level === 'safe'
  return (
    <div className="p-4 bg-green-500/10 border border-green-500/30 rounded-lg">
      <div className="flex items-start gap-3">
        <CheckCircle className="w-5 h-5 text-green-400 flex-shrink-0 mt-0.5" />
        <div className="flex-1">
          <p className="font-semibold text-green-300">Safe to Resume</p>
          <p className="text-sm text-green-200 mt-1">{recommendation}</p>
          <div className="mt-3 space-y-1">
            {report.checks_passed.slice(0, 3).map((check: string, i: number) => (
              <p key={i} className="text-xs text-green-300/80">
                ✓ {check}
              </p>
            ))}
          </div>
          <div className="mt-4 flex gap-2">
            <button
              onClick={handleResume}
              disabled={isResuming}
              className="px-4 py-2 text-sm bg-green-600 hover:bg-green-500 disabled:opacity-50 rounded text-white transition font-medium"
            >
              {isResuming ? 'Resuming...' : 'Resume Download'}
            </button>
            <button
              onClick={onCancel}
              className="px-4 py-2 text-sm bg-gray-700 hover:bg-gray-600 rounded text-gray-200 transition"
            >
              Cancel
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

interface DownloadHealthCardProps {
  compact?: boolean;
}

/**
 * Dashboard card showing overall download health
 */
export function DownloadHealthCard({ compact = false }: DownloadHealthCardProps) {
  const { getDownloadsHealthSummary } = useDownloadStateManagement();
  const [summary, setSummary] = useState<any>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const load = async () => {
      try {
        const result = await getDownloadsHealthSummary();
        setSummary(result);
      } finally {
        setLoading(false);
      }
    };
    load();
    const interval = setInterval(load, 10_000);
    return () => clearInterval(interval);
  }, [getDownloadsHealthSummary]);

  if (loading || !summary) {
    return <div className="p-4 animate-pulse bg-gray-900/50 rounded">Loading...</div>;
  }

  const { healthy_count, at_risk_count, failed_count, overall_progress_percent } = summary;
  const total = healthy_count + at_risk_count + failed_count;

  if (compact) {
    return (
      <div className="flex items-center gap-4 p-3 bg-gray-900/50 border border-gray-800 rounded text-sm">
        <div className="flex-1">
          <div className="text-xs text-gray-400">Health Status</div>
          <div className="flex gap-2 mt-1">
            {healthy_count > 0 && (
              <span className="px-2 py-1 bg-green-500/20 text-green-300 rounded text-xs">
                {healthy_count} OK
              </span>
            )}
            {at_risk_count > 0 && (
              <span className="px-2 py-1 bg-yellow-500/20 text-yellow-300 rounded text-xs">
                {at_risk_count} Risk
              </span>
            )}
            {failed_count > 0 && (
              <span className="px-2 py-1 bg-red-500/20 text-red-300 rounded text-xs">
                {failed_count} Failed
              </span>
            )}
          </div>
        </div>
        <div className="w-24 h-1.5 bg-gray-800 rounded-full overflow-hidden">
          <div
            className="h-full bg-cyan-500 transition-all duration-300"
            style={{ width: `${overall_progress_percent}%` }}
          />
        </div>
      </div>
    );
  }

  return (
    <div className="p-4 bg-gray-900/50 border border-gray-800 rounded-lg">
      <h3 className="text-sm font-semibold text-gray-200 mb-4">Download Health</h3>

      <div className="grid grid-cols-3 gap-3 mb-4">
        <div className="p-3 bg-green-500/10 border border-green-500/20 rounded">
          <div className="text-2xl font-bold text-green-400">{healthy_count}</div>
          <div className="text-xs text-green-300 mt-1">Healthy</div>
        </div>
        <div className="p-3 bg-yellow-500/10 border border-yellow-500/20 rounded">
          <div className="text-2xl font-bold text-yellow-400">{at_risk_count}</div>
          <div className="text-xs text-yellow-300 mt-1">At Risk</div>
        </div>
        <div className="p-3 bg-red-500/10 border border-red-500/20 rounded">
          <div className="text-2xl font-bold text-red-400">{failed_count}</div>
          <div className="text-xs text-red-300 mt-1">Failed</div>
        </div>
      </div>

      <div className="mb-3">
        <div className="flex justify-between items-center mb-2">
          <div className="text-xs text-gray-400">Overall Progress</div>
          <div className="text-sm font-semibold text-gray-300">{overall_progress_percent}%</div>
        </div>
        <div className="w-full h-2 bg-gray-800 rounded-full overflow-hidden">
          <div
            className="h-full bg-gradient-to-r from-cyan-500 to-blue-500 transition-all duration-300"
            style={{ width: `${overall_progress_percent}%` }}
          />
        </div>
      </div>

      <div className="text-xs text-gray-500">
        {total} total downloads • Last updated: just now
      </div>
    </div>
  );
}

interface DownloadStateDisplayProps {
  downloadId: string;
}

/**
 * Displays state information for a single download
 */
export function DownloadStateDisplay({ downloadId }: DownloadStateDisplayProps) {
  const { getDownloadState, getDownloadDiagnostics } = useDownloadStateManagement();
  const [state, setState] = useState<any>(null);
  const [diagnostics, setDiagnostics] = useState<any>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const load = async () => {
      try {
        const [stateData, diagData] = await Promise.all([
          getDownloadState(downloadId),
          getDownloadDiagnostics(downloadId),
        ]);
        setState(stateData);
        setDiagnostics(diagData);
      } finally {
        setLoading(false);
      }
    };
    load();
  }, [downloadId, getDownloadState, getDownloadDiagnostics]);

  if (loading || !state) {
    return <div className="text-sm text-gray-500">Loading state...</div>;
  }

  const statusColor = getStatusColor(state.status);
  const statusLabel = getStatusLabel(state.status);

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <div>
          <p className="text-xs text-gray-500">Current State</p>
          <span className={`inline-block px-3 py-1 rounded text-xs font-semibold mt-1 ${statusColor}`}>
            {statusLabel}
          </span>
        </div>
        {diagnostics && (
          <span
            className={`inline-block px-3 py-1 rounded text-xs font-semibold ${
              diagnostics.is_healthy
                ? 'bg-green-500/20 text-green-300'
                : 'bg-red-500/20 text-red-300'
            }`}
          >
            {diagnostics.is_healthy ? '✓ Healthy' : '⚠ Issues'}
          </span>
        )}
      </div>

      <div className="text-sm">
        <p className="text-gray-400">
          <span className="text-gray-500">{state.downloaded_bytes} / {state.total_size} bytes</span>
        </p>
        <p className="text-gray-400 mt-1">
          <span className="text-gray-500">{state.progress_percent}% complete</span>
        </p>
      </div>

      {state.error_message && (
        <div className="p-2 bg-red-500/10 border border-red-500/30 rounded text-xs text-red-300">
          {state.error_message}
        </div>
      )}

      {diagnostics?.recommendation && (
        <div className="p-2 bg-blue-500/10 border border-blue-500/30 rounded text-xs text-blue-300">
          {diagnostics.recommendation}
        </div>
      )}
    </div>
  );
}
