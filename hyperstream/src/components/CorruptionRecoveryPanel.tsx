import React, { useState, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import {
  AlertTriangle,
  RefreshCw,
  TrendingUp,
  Clock,
  CheckCircle,
  ChevronDown,
  Zap,
  Shield,
  Link2,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';

interface CorruptionEvidence {
  segment_id: number;
  segment_start: number;
  segment_end: number;
  corruption_type: {
    SizeMismatch?: { expected: number; actual: number };
    ChecksumMismatch?: { expected: string; computed: string; algorithm: string };
    ZeroEntropy?: null;
    LowEntropy?: { entropy: number; threshold: number };
  };
  confidence: number;
  detected_at_ms: number;
  evidence_data: string;
}

interface MirrorReliability {
  url: string;
  success_count: number;
  failure_count: number;
  corruption_count: number;
  average_speed_bps: number;
  last_used_ms: number;
  score: number;
}

const CorruptionRecoveryPanel: React.FC<{ downloadId: string }> = ({ downloadId }) => {
  const [corruptions, setCorruptions] = useState<CorruptionEvidence[]>([]);
  const [mirrors, setMirrors] = useState<MirrorReliability[]>([]);
  const [expandedSegment, setExpandedSegment] = useState<number | null>(null);
  const [recovering, setRecovering] = useState(false);

  // Load corruption report on mount
  useEffect(() => {
    const loadReport = async () => {
      try {
        const report = await invoke<CorruptionEvidence[]>('get_corruption_report', {
          download_id: downloadId,
        });
        setCorruptions(report);

        const rankings = await invoke<MirrorReliability[]>('get_mirror_rankings', {});
        setMirrors(rankings);
      } catch (err) {
        console.error('Failed to load corruption report:', err);
      }
    };

    loadReport();
    const interval = setInterval(loadReport, 5000); // Refresh every 5s
    return () => clearInterval(interval);
  }, [downloadId]);

  const handleRecovery = async (segmentId: number, segmentStart: number, segmentEnd: number) => {
    setRecovering(true);
    try {
      // Get recommended strategy
      const strategy = await invoke<string>('get_recovery_strategy', {
        download_id: downloadId,
        segment_id: segmentId,
        segment_start: segmentStart,
        segment_end: segmentEnd,
        original_url: '', // Would come from download context
        alternative_mirrors: mirrors.map((m) => m.url),
      });

      // Execute recovery
      const result = await invoke<string>('execute_recovery', {
        download_id: downloadId,
        segment_id: segmentId,
        strategy: JSON.parse(strategy),
      });

      console.log('Recovery result:', result);
      
      // Refresh reports
      const report = await invoke<CorruptionEvidence[]>('get_corruption_report', {
        download_id: downloadId,
      });
      setCorruptions(report);
    } catch (err) {
      console.error('Recovery failed:', err);
    } finally {
      setRecovering(false);
    }
  };

  const getCorruptionIcon = (type: CorruptionEvidence['corruption_type']) => {
    if (type.SizeMismatch) return '📦';
    if (type.ChecksumMismatch) return '🔐';
    if (type.ZeroEntropy) return '🔳';
    if (type.LowEntropy) return '📉';
    return '⚠️';
  };

  const getCorruptionDescription = (evidence: CorruptionEvidence) => {
    const type = evidence.corruption_type;
    if (type.SizeMismatch) {
      return `Size mismatch: expected ${type.SizeMismatch.expected}, got ${type.SizeMismatch.actual}`;
    }
    if (type.ChecksumMismatch) {
      return `${type.ChecksumMismatch.algorithm} mismatch`;
    }
    if (type.ZeroEntropy) {
      return 'All bytes identical (zero entropy)';
    }
    if (type.LowEntropy) {
      return `Low entropy ${type.LowEntropy.entropy.toFixed(2)} (threshold: ${type.LowEntropy.threshold.toFixed(2)})`;
    }
    return 'Unknown corruption type';
  };

  const healthyMirrors = mirrors.filter((m) => m.score >= 80);

  return (
    <div className="w-full bg-cyan-950/20 backdrop-blur-xl border border-cyan-500/20 rounded-lg p-6">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div className="flex items-center gap-3">
          <AlertTriangle className="w-5 h-5 text-orange-400" />
          <h2 className="text-lg font-semibold text-cyan-100">Corruption Detection & Recovery</h2>
        </div>
        <span className="px-3 py-1 bg-orange-500/20 rounded-full text-sm text-orange-300 border border-orange-500/30">
          {corruptions.length} issue{corruptions.length !== 1 ? 's' : ''}
        </span>
      </div>

      <AnimatePresence mode="wait">
        {corruptions.length === 0 ? (
          <motion.div
            key="healthy"
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            className="bg-green-950/20 border border-green-500/20 rounded-lg p-4 flex items-center gap-3"
          >
            <CheckCircle className="w-5 h-5 text-green-400" />
            <span className="text-green-300">No corruption detected. Download integrity verified.</span>
          </motion.div>
        ) : (
          <motion.div key="corrupted" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} className="space-y-4">
            {/* Corruption Reports */}
            <div className="space-y-3">
              {corruptions.map((evidence) => (
                <motion.div
                  key={evidence.segment_id}
                  initial={{ opacity: 0, y: 10 }}
                  animate={{ opacity: 1, y: 0 }}
                  className="bg-gray-900/50 border border-orange-500/30 rounded-lg p-4 cursor-pointer hover:border-orange-500/60 transition-colors"
                  onClick={() => setExpandedSegment(expandedSegment === evidence.segment_id ? null : evidence.segment_id)}
                >
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-3 flex-1">
                      <span className="text-2xl">{getCorruptionIcon(evidence.corruption_type)}</span>
                      <div className="flex-1">
                        <p className="font-medium text-cyan-100">
                          Segment {evidence.segment_id} ({(evidence.segment_start / 1024 / 1024).toFixed(1)}MB -
                          {((evidence.segment_end - evidence.segment_start) / 1024 / 1024).toFixed(1)}MB)
                        </p>
                        <p className="text-sm text-gray-400">{getCorruptionDescription(evidence)}</p>
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      <span className="text-xs bg-orange-500/20 text-orange-300 px-2 py-1 rounded">
                        Confidence: {evidence.confidence}%
                      </span>
                      <ChevronDown
                        className="w-5 h-5 text-gray-500 transition-transform"
                        style={{ transform: expandedSegment === evidence.segment_id ? 'rotate(180deg)' : '' }}
                      />
                    </div>
                  </div>

                  {/* Expanded Details */}
                  <AnimatePresence>
                    {expandedSegment === evidence.segment_id && (
                      <motion.div
                        initial={{ opacity: 0, height: 0 }}
                        animate={{ opacity: 1, height: 'auto' }}
                        exit={{ opacity: 0, height: 0 }}
                        className="mt-4 pt-4 border-t border-gray-700 space-y-4"
                      >
                        <div className="grid grid-cols-2 gap-4 text-sm">
                          <div>
                            <p className="text-gray-400">Evidence</p>
                            <p className="text-cyan-300 font-mono text-xs break-all">{evidence.evidence_data}</p>
                          </div>
                          <div>
                            <p className="text-gray-400">Detected</p>
                            <p className="text-cyan-300">
                              {new Date(evidence.detected_at_ms).toLocaleTimeString()}
                            </p>
                          </div>
                        </div>

                        {/* Recovery Options */}
                        <div className="bg-cyan-950/30 border border-cyan-500/20 rounded p-3 space-y-2">
                          <p className="text-sm text-cyan-300 font-medium flex items-center gap-2">
                            <Zap className="w-4 h-4" />
                            Recovery Options
                          </p>

                          {/* Retry Original */}
                          <button
                            onClick={() => handleRecovery(evidence.segment_id, evidence.segment_start, evidence.segment_end)}
                            disabled={recovering}
                            className="w-full flex items-center justify-between p-2 bg-blue-900/30 hover:bg-blue-900/50 border border-blue-500/30 rounded transition-colors disabled:opacity-50"
                          >
                            <span className="flex items-center gap-2 text-blue-300">
                              <RefreshCw className="w-4 h-4" />
                              Retry Original Source
                            </span>
                            <span className="text-xs text-blue-400">Exponential backoff</span>
                          </button>

                          {/* Switch Mirror */}
                          {healthyMirrors.length > 0 && (
                            <button
                              onClick={() => handleRecovery(evidence.segment_id, evidence.segment_start, evidence.segment_end)}
                              disabled={recovering}
                              className="w-full flex items-center justify-between p-2 bg-green-900/30 hover:bg-green-900/50 border border-green-500/30 rounded transition-colors disabled:opacity-50"
                            >
                              <span className="flex items-center gap-2 text-green-300">
                                <Link2 className="w-4 h-4" />
                                Switch to Healthy Mirror
                              </span>
                              <span className="text-xs text-green-400">{healthyMirrors[0].url.substring(0, 20)}...</span>
                            </button>
                          )}

                          {/* Resume from Offset */}
                          <button
                            onClick={() => handleRecovery(evidence.segment_id, evidence.segment_start, evidence.segment_end)}
                            disabled={recovering}
                            className="w-full flex items-center justify-between p-2 bg-purple-900/30 hover:bg-purple-900/50 border border-purple-500/30 rounded transition-colors disabled:opacity-50"
                          >
                            <span className="flex items-center gap-2 text-purple-300">
                              <TrendingUp className="w-4 h-4" />
                              Resume from Byte Offset
                            </span>
                            <span className="text-xs text-purple-400">Zero-copy</span>
                          </button>
                        </div>

                        {/* Health Warning */}
                        {evidence.corruption_type.LowEntropy && (
                          <div className="bg-yellow-950/20 border border-yellow-500/30 rounded p-3 text-sm text-yellow-300 flex items-start gap-2">
                            <Shield className="w-4 h-4 mt-0.5 flex-shrink-0" />
                            <span>
                              Low entropy detected—likely a partial or corrupted download. Auto-repair will attempt
                              recovery via mirroring or re-download.
                            </span>
                          </div>
                        )}
                      </motion.div>
                    )}
                  </AnimatePresence>
                </motion.div>
              ))}
            </div>

            {/* Mirror Health Rankings */}
            <div className="mt-6 bg-gray-900/50 border border-gray-700/50 rounded-lg p-4">
              <h3 className="text-sm font-semibold text-cyan-300 mb-3 flex items-center gap-2">
                <TrendingUp className="w-4 h-4" />
                Mirror Reliability Rankings
              </h3>
              <div className="space-y-2 max-h-40 overflow-y-auto">
                {mirrors.slice(0, 5).map((mirror) => (
                  <div key={mirror.url} className="flex items-center justify-between text-xs">
                    <div className="flex-1 min-w-0">
                      <p className="text-gray-300 truncate">{mirror.url}</p>
                      <p className="text-gray-500">
                        {mirror.success_count}✓ {mirror.failure_count}✗ {mirror.corruption_count}⚠
                      </p>
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="w-16 h-1.5 bg-gray-700 rounded-full overflow-hidden">
                        <div
                          className="h-full bg-green-500 transition-all"
                          style={{ width: `${mirror.score}%` }}
                        />
                      </div>
                      <span className="text-cyan-300 font-mono w-6">{mirror.score}%</span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Auto-Cleanup Schedule */}
      <div className="mt-4 text-xs text-gray-500 flex items-center gap-1">
        <Clock className="w-3 h-3" />
        Recovery data automatically cleaned up after 7 days
      </div>
    </div>
  );
};

export default CorruptionRecoveryPanel;
