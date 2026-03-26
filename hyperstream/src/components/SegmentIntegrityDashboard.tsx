import React, { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import {
  AlertTriangle, CheckCircle, Shield, TrendingUp, Zap,
  RefreshCw, BarChart3, Layers3, Clock, Droplet
} from 'lucide-react';

interface SegmentIntegrityInfo {
  segment_id: number;
  start_byte: number;
  end_byte: number;
  expected_size: number;
  actual_size: number;
  size_valid: boolean;
  checksum: string | null;
  expected_checksum: string | null;
  checksum_valid: boolean;
  entropy: number;
  appears_corrupted: boolean;
  integrity_score: number;
  verified_at_ms: number;
  verification_duration_ms: number;
}

interface IntegrityReport {
  download_id: string;
  file_path: string;
  total_size: number;
  segments: SegmentIntegrityInfo[];
  failed_segments: number[];
  overall_score: number;
  risk_level: 'Healthy' | 'Caution' | 'Warning' | 'Critical';
  at_risk_percentage: number;
  recommendations: string[];
  generated_at_ms: number;
  total_duration_ms: number;
  parallel_degree: number;
}

interface IntegrityMetrics {
  total_segments_verified: number;
  total_corruptions_detected: number;
  auto_recovery_attempts: number;
  auto_recovery_success: number;
  average_verification_time_ms: number;
  average_integrity_score: number;
}

interface DownloadSegmentHealth {
  download_id: string;
  overall_score: number;
  risk_level: string;
  at_risk_percentage: number;
  failed_segments_count: number;
  total_segments: number;
  is_healthy: boolean;
  can_resume: boolean;
  should_restart: boolean;
  verification_time_ms: number;
  global_metrics: IntegrityMetrics;
  recommendations: string[];
}

export const SegmentIntegrityDashboard: React.FC<{ downloadId: string }> = ({ downloadId }) => {
  const [report, setReport] = useState<IntegrityReport | null>(null);
  const [summary, setSummary] = useState<DownloadSegmentHealth | null>(null);
  const [loading, setLoading] = useState(false);
  const [verifying, setVerifying] = useState(false);
  const [expandedSegments, setExpandedSegments] = useState<Set<number>>(new Set());

  const fetchIntegrityReport = useCallback(async () => {
    setLoading(true);
    try {
      const [reportData, summaryData] = await Promise.all([
        invoke<IntegrityReport>('get_cached_integrity_report', { downloadId }),
        invoke<DownloadSegmentHealth>('get_integrity_summary', { downloadId }).catch(() => null),
      ]);

      if (reportData) {
        setReport(reportData);
      }
      if (summaryData) {
        setSummary(summaryData);
      }
    } catch (error) {
      console.error('Failed to fetch integrity report:', error);
    } finally {
      setLoading(false);
    }
  }, [downloadId]);

  const verifyIntegrity = useCallback(async () => {
    setVerifying(true);
    try {
      const reportData = await invoke<IntegrityReport>('verify_download_integrity', { downloadId });
      setReport(reportData);
      
      // Fetch summary after verification
      const summaryData = await invoke<DownloadSegmentHealth>('get_integrity_summary', { downloadId }).catch(() => null);
      if (summaryData) {
        setSummary(summaryData);
      }
    } catch (error) {
      console.error('Verification failed:', error);
    } finally {
      setVerifying(false);
    }
  }, [downloadId]);

  useEffect(() => {
    fetchIntegrityReport();
    const interval = setInterval(fetchIntegrityReport, 5000);
    return () => clearInterval(interval);
  }, [downloadId, fetchIntegrityReport]);

  const getRiskColor = (level: string) => {
    switch (level) {
      case 'Healthy':
        return 'text-green-400';
      case 'Caution':
        return 'text-yellow-400';
      case 'Warning':
        return 'text-orange-400';
      case 'Critical':
        return 'text-red-400';
      default:
        return 'text-slate-400';
    }
  };

  const getRiskBg = (level: string) => {
    switch (level) {
      case 'Healthy':
        return 'bg-green-500/10 border-green-500/20';
      case 'Caution':
        return 'bg-yellow-500/10 border-yellow-500/20';
      case 'Warning':
        return 'bg-orange-500/10 border-orange-500/20';
      case 'Critical':
        return 'bg-red-500/10 border-red-500/20';
      default:
        return 'bg-slate-500/10 border-slate-500/20';
    }
  };

  const getScoreColor = (score: number) => {
    if (score >= 90) return 'text-green-400';
    if (score >= 70) return 'text-yellow-400';
    if (score >= 50) return 'text-orange-400';
    return 'text-red-400';
  };

  const toggleSegment = (segmentId: number) => {
    const newExpanded = new Set(expandedSegments);
    if (newExpanded.has(segmentId)) {
      newExpanded.delete(segmentId);
    } else {
      newExpanded.add(segmentId);
    }
    setExpandedSegments(newExpanded);
  };

  if (loading && !report) {
    return (
      <div className="flex items-center justify-center p-8">
        <div className="animate-spin">
          <RefreshCw size={24} className="text-blue-400" />
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header with Action Button */}
      <motion.div
        className="flex items-center justify-between"
        initial={{ opacity: 0, y: -20 }}
        animate={{ opacity: 1, y: 0 }}
      >
        <div className="flex items-center gap-3">
          <Shield size={24} className="text-blue-400" />
          <h2 className="text-2xl font-bold text-slate-100">Segment Integrity Verification</h2>
        </div>
        <button
          onClick={verifyIntegrity}
          disabled={verifying}
          className="flex items-center gap-2 px-4 py-2 bg-blue-500/20 hover:bg-blue-500/30 border border-blue-500/50 rounded-lg text-blue-300 transition-colors disabled:opacity-50"
        >
          {verifying ? (
            <>
              <RefreshCw size={16} className="animate-spin" />
              Verifying...
            </>
          ) : (
            <>
              <Zap size={16} />
              Verify Now
            </>
          )}
        </button>
      </motion.div>

      {/* Overall Status Card */}
      {summary && (
        <motion.div
          className={`${getRiskBg(summary.risk_level)} border rounded-xl p-6 backdrop-blur-xl`}
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
        >
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <div>
              <div className="text-sm text-slate-400 mb-1">Integrity Score</div>
              <div className={`text-3xl font-bold ${getScoreColor(summary.overall_score)}`}>
                {summary.overall_score}%
              </div>
            </div>
            <div>
              <div className="text-sm text-slate-400 mb-1">Risk Level</div>
              <div className={`text-lg font-bold ${getRiskColor(summary.risk_level)}`}>
                {summary.risk_level}
              </div>
            </div>
            <div>
              <div className="text-sm text-slate-400 mb-1">At-Risk Data</div>
              <div className="text-lg font-bold text-orange-400">
                {(summary.at_risk_percentage * 100).toFixed(1)}%
              </div>
            </div>
            <div>
              <div className="text-sm text-slate-400 mb-1">Verification Time</div>
              <div className="text-lg font-bold text-blue-400 flex items-center gap-1">
                <Clock size={16} />
                {summary.verification_time_ms}ms
              </div>
            </div>
          </div>

          {/* Status Indicators */}
          <div className="grid grid-cols-3 gap-2 mt-4">
            <div className="flex items-center gap-2 text-sm">
              {summary.is_healthy ? (
                <CheckCircle size={16} className="text-green-400" />
              ) : (
                <AlertTriangle size={16} className="text-yellow-400" />
              )}
              <span>{summary.is_healthy ? 'Healthy' : 'Monitor'}</span>
            </div>
            <div className="flex items-center gap-2 text-sm">
              {summary.can_resume ? (
                <CheckCircle size={16} className="text-green-400" />
              ) : (
                <AlertTriangle size={16} className="text-red-400" />
              )}
              <span>{summary.can_resume ? 'Resumable' : 'Not Safe'}</span>
            </div>
            <div className="flex items-center gap-2 text-sm">
              {!summary.should_restart ? (
                <CheckCircle size={16} className="text-green-400" />
              ) : (
                <AlertTriangle size={16} className="text-orange-400" />
              )}
              <span>{summary.should_restart ? 'Restart' : 'OK'}</span>
            </div>
          </div>
        </motion.div>
      )}

      {/* Recommendations */}
      {summary && summary.recommendations.length > 0 && (
        <motion.div
          className="bg-blue-500/10 border border-blue-500/20 rounded-xl p-4 backdrop-blur-xl"
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
        >
          <div className="flex items-start gap-3">
            <TrendingUp size={20} className="text-blue-400 flex-shrink-0 mt-1" />
            <div>
              <h3 className="font-semibold text-blue-300 mb-2">Recommendations</h3>
              <ul className="space-y-1">
                {summary.recommendations.map((rec, idx) => (
                  <li key={idx} className="text-sm text-slate-300 flex items-start gap-2">
                    <span className="text-blue-400 mt-0.5">•</span>
                    <span>{rec}</span>
                  </li>
                ))}
              </ul>
            </div>
          </div>
        </motion.div>
      )}

      {/* Segment Details */}
      {report && (
        <motion.div
          className="bg-slate-900/40 border border-slate-700/50 rounded-xl p-6 backdrop-blur-xl"
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
        >
          <div className="flex items-center justify-between mb-4">
            <h3 className="font-semibold text-slate-200 flex items-center gap-2">
              <Layers3 size={18} className="text-slate-400" />
              Segment Analysis ({report.segments.length} segments)
            </h3>
            <div className="text-sm text-slate-400">
              {report.failed_segments.length} at-risk
            </div>
          </div>

          <div className="space-y-2 max-h-96 overflow-y-auto">
            <AnimatePresence>
              {report.segments.map((seg) => (
                <motion.div
                  key={seg.segment_id}
                  className={`border rounded-lg p-3 cursor-pointer transition-all ${
                    seg.appears_corrupted
                      ? 'bg-red-500/10 border-red-500/30'
                      : seg.integrity_score >= 90
                      ? 'bg-green-500/10 border-green-500/20'
                      : 'bg-yellow-500/10 border-yellow-500/20'
                  }`}
                  onClick={() => toggleSegment(seg.segment_id)}
                  layout
                >
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-3 flex-1">
                      {seg.appears_corrupted ? (
                        <AlertTriangle size={16} className="text-red-400 flex-shrink-0" />
                      ) : (
                        <CheckCircle size={16} className="text-green-400 flex-shrink-0" />
                      )}
                      <div>
                        <div className="font-medium text-slate-200">
                          Segment {seg.segment_id}
                        </div>
                        <div className="text-xs text-slate-400">
                          {(seg.start_byte / 1024 / 1024).toFixed(2)}MB - {(seg.end_byte / 1024 / 1024).toFixed(2)}MB
                        </div>
                      </div>
                    </div>
                    <div className="flex items-center gap-4">
                      <div className="text-right">
                        <div className={`font-bold ${getScoreColor(seg.integrity_score)}`}>
                          {seg.integrity_score}%
                        </div>
                      </div>
                      <Droplet
                        size={16}
                        className={expandedSegments.has(seg.segment_id) ? 'rotate-180' : ''}
                        style={{ transform: expandedSegments.has(seg.segment_id) ? 'rotate(180deg)' : '' }}
                      />
                    </div>
                  </div>

                  {/* Expanded Details */}
                  <AnimatePresence>
                    {expandedSegments.has(seg.segment_id) && (
                      <motion.div
                        className="mt-3 pt-3 border-t border-white/10 space-y-2 text-sm"
                        initial={{ opacity: 0, height: 0 }}
                        animate={{ opacity: 1, height: 'auto' }}
                        exit={{ opacity: 0, height: 0 }}
                      >
                        <div className="grid grid-cols-2 gap-2">
                          <div>
                            <span className="text-slate-400">Size:</span>
                            <span className="text-slate-200 ml-2">
                              {seg.actual_size} / {seg.expected_size} bytes
                            </span>
                          </div>
                          <div>
                            <span className="text-slate-400">Entropy:</span>
                            <span className="text-slate-200 ml-2">
                              {(seg.entropy * 100).toFixed(1)}%
                            </span>
                          </div>
                        </div>
                        <div>
                          <span className="text-slate-400">Checksum:</span>
                          <span className="text-slate-200 ml-2 font-mono text-xs">
                            {seg.checksum ? seg.checksum.substring(0, 16) + '...' : 'N/A'}
                          </span>
                        </div>
                        <div>
                          <span className="text-slate-400">Verified:</span>
                          <span className="text-slate-200 ml-2">
                            {seg.verification_duration_ms}ms ago
                          </span>
                        </div>
                      </motion.div>
                    )}
                  </AnimatePresence>
                </motion.div>
              ))}
            </AnimatePresence>
          </div>
        </motion.div>
      )}

      {/* Global Metrics */}
      {summary && (
        <motion.div
          className="bg-slate-900/40 border border-slate-700/50 rounded-xl p-6 backdrop-blur-xl"
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
        >
          <h3 className="font-semibold text-slate-200 flex items-center gap-2 mb-4">
            <BarChart3 size={18} className="text-slate-400" />
            System Metrics
          </h3>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <div>
              <div className="text-xs text-slate-400 mb-1">Total Verified</div>
              <div className="text-lg font-bold text-blue-400">
                {summary.global_metrics.total_segments_verified}
              </div>
            </div>
            <div>
              <div className="text-xs text-slate-400 mb-1">Corruptions</div>
              <div className="text-lg font-bold text-red-400">
                {summary.global_metrics.total_corruptions_detected}
              </div>
            </div>
            <div>
              <div className="text-xs text-slate-400 mb-1">Avg Score</div>
              <div className={`text-lg font-bold ${getScoreColor(summary.global_metrics.average_integrity_score)}`}>
                {summary.global_metrics.average_integrity_score.toFixed(1)}%
              </div>
            </div>
            <div>
              <div className="text-xs text-slate-400 mb-1">Avg Time</div>
              <div className="text-lg font-bold text-slate-300">
                {summary.global_metrics.average_verification_time_ms.toFixed(0)}ms
              </div>
            </div>
          </div>
        </motion.div>
      )}
    </div>
  );
};
