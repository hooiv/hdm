import React, { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { AlertCircle, CheckCircle, TrendingDown, TrendingUp, Zap, Loader2, Copy, Download } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';

interface PreFlightAnalysis {
  url: string;
  analysis_timestamp_ms: number;
  analysis_duration_ms: number;
  file_name?: string;
  file_size_bytes?: number;
  content_type?: string;
  last_modified?: string;
  detected_mirrors: MirrorInfo[];
  primary_mirror?: MirrorInfo;
  fallback_mirrors: MirrorInfo[];
  connection_health: 'Excellent' | 'Good' | 'Fair' | 'Poor' | 'Unreachable';
  dns_latency_ms?: number;
  tcp_latency_ms?: number;
  tls_latency_ms?: number;
  pre_test_speed_mbps?: number;
  reliability_score: number;
  availability_score: number;
  success_probability: number;
  estimated_speed_mbps: number;
  risk_factors: string[];
  risk_level: 'Safe' | 'Low' | 'Medium' | 'High' | 'Critical';
  recommendations: Recommendation[];
  optimal_strategy: string;
  estimated_duration_seconds?: number;
  mirror_success_rates: Record<string, number>;
  mirror_avg_speeds: Record<string, number>;
}

interface MirrorInfo {
  url: string;
  host: string;
  protocol: string;
  location?: string;
  is_cdn: boolean;
  health_score: number;
  last_checked_ms: number;
}

interface Recommendation {
  category: string;
  suggestion: string;
  expected_benefit: string;
  priority: number;
}

const PreFlightAnalysisDashboard: React.FC = () => {
  const [urlInput, setUrlInput] = useState('');
  const [analysis, setAnalysis] = useState<PreFlightAnalysis | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expandedMirror, setExpandedMirror] = useState<string | null>(null);
  const [showSummary, setShowSummary] = useState(false);

  const handleAnalyze = useCallback(async () => {
    if (!urlInput.trim()) {
      setError('Please enter a URL');
      return;
    }

    setLoading(true);
    setError(null);
    setAnalysis(null);

    try {
      const result = await invoke<PreFlightAnalysis>('analyze_url_preflight', {
        url: urlInput.trim(),
      });
      setAnalysis(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [urlInput]);

  const handleGetSummary = useCallback(async () => {
    if (!analysis) return;

    try {
      const summary = await invoke<string>('get_preflight_analysis_summary', {
        analysis,
      });
      // Copy to clipboard and show notification
      await navigator.clipboard.writeText(summary);
      alert('Summary copied to clipboard!');
    } catch (err) {
      console.error('Failed to get summary:', err);
    }
  }, [analysis]);

  const getRiskColor = (risk: string) => {
    switch (risk) {
      case 'Safe':
        return 'from-green-500/20 to-green-600/20 border border-green-500/50';
      case 'Low':
        return 'from-blue-500/20 to-blue-600/20 border border-blue-500/50';
      case 'Medium':
        return 'from-yellow-500/20 to-yellow-600/20 border border-yellow-500/50';
      case 'High':
        return 'from-orange-500/20 to-orange-600/20 border border-orange-500/50';
      case 'Critical':
        return 'from-red-500/20 to-red-600/20 border border-red-500/50';
      default:
        return 'from-slate-500/20 to-slate-600/20';
    }
  };

  const getHealthColor = (health: string) => {
    switch (health) {
      case 'Excellent':
        return 'text-green-400';
      case 'Good':
        return 'text-blue-400';
      case 'Fair':
        return 'text-yellow-400';
      case 'Poor':
        return 'text-orange-400';
      case 'Unreachable':
        return 'text-red-400';
      default:
        return 'text-slate-400';
    }
  };

  const getRiskIcon = (risk: string) => {
    switch (risk) {
      case 'Safe':
        return <CheckCircle className="w-5 h-5 text-green-400" />;
      case 'Critical':
        return <AlertCircle className="w-5 h-5 text-red-400" />;
      default:
        return <Zap className="w-5 h-5 text-yellow-400" />;
    }
  };

  const formatBytes = (bytes: number): string => {
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let size = bytes;
    let unitIdx = 0;

    while (size >= 1024 && unitIdx < units.length - 1) {
      size /= 1024;
      unitIdx++;
    }

    return unitIdx === 0 ? `${bytes} ${units[0]}` : `${size.toFixed(2)} ${units[unitIdx]}`;
  };

  const formatDuration = (seconds: number): string => {
    if (seconds < 60) return `${seconds} sec`;
    if (seconds < 3600) {
      const mins = Math.floor(seconds / 60);
      const secs = seconds % 60;
      return `${mins}m ${secs}s`;
    }
    const hours = Math.floor(seconds / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    return `${hours}h ${mins}m`;
  };

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-900 to-slate-800 p-8">
      <div className="max-w-7xl mx-auto">
        {/* Header */}
        <motion.div
          initial={{ opacity: 0, y: -20 }}
          animate={{ opacity: 1, y: 0 }}
          className="mb-8"
        >
          <div className="flex items-center gap-3 mb-2">
            <Zap className="w-8 h-8 text-cyan-400" />
            <h1 className="text-4xl font-bold bg-gradient-to-r from-cyan-400 to-blue-500 bg-clip-text text-transparent">
              Pre-Flight Analysis
            </h1>
          </div>
          <p className="text-slate-400 text-lg">
            Intelligent URL analysis before downloading — Know what you're getting into before you start
          </p>
        </motion.div>

        {/* Input Section */}
        <motion.div
          initial={{ opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.1 }}
          className="mb-8"
        >
          <div className="flex gap-3">
            <input
              type="text"
              placeholder="Paste URL to analyze..."
              value={urlInput}
              onChange={(e) => setUrlInput(e.target.value)}
              onKeyPress={(e) => e.key === 'Enter' && handleAnalyze()}
              className="flex-1 px-4 py-3 bg-slate-800/50 border border-slate-700 rounded-lg text-slate-200 placeholder-slate-500 focus:outline-none focus:border-cyan-500 focus:ring-1 focus:ring-cyan-500"
            />
            <button
              onClick={handleAnalyze}
              disabled={loading}
              className="px-6 py-3 bg-gradient-to-r from-cyan-500 to-blue-600 hover:from-cyan-400 hover:to-blue-500 disabled:from-slate-600 disabled:to-slate-700 text-white font-semibold rounded-lg transition-all duration-200 flex items-center gap-2"
            >
              {loading ? (
                <>
                  <Loader2 className="w-4 h-4 animate-spin" />
                  Analyzing...
                </>
              ) : (
                <>
                  <Zap className="w-4 h-4" />
                  Analyze
                </>
              )}
            </button>
          </div>
        </motion.div>

        {/* Error Display */}
        <AnimatePresence>
          {error && (
            <motion.div
              initial={{ opacity: 0, y: -10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -10 }}
              className="mb-8 p-4 bg-red-500/20 border border-red-500/50 rounded-lg text-red-400 flex gap-3"
            >
              <AlertCircle className="w-5 h-5 flex-shrink-0 mt-0.5" />
              <div>
                <p className="font-semibold">Analysis failed</p>
                <p className="text-sm text-red-300">{error}</p>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        {/* Analysis Results */}
        <AnimatePresence>
          {analysis && (
            <motion.div
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -10 }}
              className="space-y-6"
            >
              {/* Risk Assessment Card */}
              <motion.div
                className={`bg-gradient-to-br ${getRiskColor(analysis.risk_level)} rounded-xl p-6 backdrop-blur-sm`}
                whileHover={{ scale: 1.02 }}
              >
                <div className="flex items-start justify-between mb-4">
                  <div className="flex items-center gap-3">
                    {getRiskIcon(analysis.risk_level)}
                    <div>
                      <h2 className="text-2xl font-bold text-slate-100">Risk Assessment</h2>
                      <p className="text-slate-400 text-sm">
                        Success Probability: {(analysis.success_probability * 100).toFixed(1)}%
                      </p>
                    </div>
                  </div>
                  <div className="text-right">
                    <div className="text-3xl font-bold text-slate-100">
                      {analysis.risk_level}
                    </div>
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-4 mb-4">
                  <div>
                    <p className="text-slate-400 text-sm mb-1">Reliability</p>
                    <div className="flex items-center gap-2">
                      <div className="flex-1 h-2 bg-slate-700/50 rounded-full overflow-hidden">
                        <div
                          className="h-full bg-gradient-to-r from-cyan-500 to-blue-500"
                          style={{ width: `${analysis.reliability_score}%` }}
                        />
                      </div>
                      <span className="text-slate-300 font-semibold w-12 text-right">
                        {analysis.reliability_score.toFixed(0)}%
                      </span>
                    </div>
                  </div>
                  <div>
                    <p className="text-slate-400 text-sm mb-1">Availability</p>
                    <div className="flex items-center gap-2">
                      <div className="flex-1 h-2 bg-slate-700/50 rounded-full overflow-hidden">
                        <div
                          className="h-full bg-gradient-to-r from-green-500 to-emerald-500"
                          style={{ width: `${analysis.availability_score}%` }}
                        />
                      </div>
                      <span className="text-slate-300 font-semibold w-12 text-right">
                        {analysis.availability_score.toFixed(0)}%
                      </span>
                    </div>
                  </div>
                </div>

                {/* Risk Factors */}
                {analysis.risk_factors.length > 0 && (
                  <div className="pt-4 border-t border-slate-600/50">
                    <p className="text-slate-300 font-semibold text-sm mb-2">⚠️ Risk Factors:</p>
                    <ul className="space-y-1">
                      {analysis.risk_factors.map((factor, idx) => (
                        <li key={idx} className="text-slate-400 text-sm flex items-start gap-2">
                          <span className="text-yellow-400 mt-0.5">•</span>
                          <span>{factor}</span>
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
              </motion.div>

              {/* File & Metadata Card */}
              <motion.div
                className="bg-gradient-to-br from-slate-800/50 to-slate-900/50 border border-slate-700 rounded-xl p-6 backdrop-blur-sm"
                whileHover={{ scale: 1.02 }}
              >
                <h3 className="text-xl font-bold text-slate-100 mb-4 flex items-center gap-2">
                  <Download className="w-5 h-5 text-cyan-400" />
                  File Information
                </h3>
                <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
                  <div className="bg-slate-700/30 rounded-lg p-3">
                    <p className="text-slate-400 text-xs uppercase font-semibold mb-1">Filename</p>
                    <p className="text-slate-200 font-mono text-sm break-all">
                      {analysis.file_name || 'Unknown'}
                    </p>
                  </div>
                  <div className="bg-slate-700/30 rounded-lg p-3">
                    <p className="text-slate-400 text-xs uppercase font-semibold mb-1">File Size</p>
                    <p className="text-slate-200 font-semibold text-sm">
                      {analysis.file_size_bytes ? formatBytes(analysis.file_size_bytes) : 'Unknown'}
                    </p>
                  </div>
                  <div className="bg-slate-700/30 rounded-lg p-3">
                    <p className="text-slate-400 text-xs uppercase font-semibold mb-1">Est. Duration</p>
                    <p className="text-slate-200 font-semibold text-sm">
                      {analysis.estimated_duration_seconds
                        ? formatDuration(analysis.estimated_duration_seconds)
                        : 'Calculating...'}
                    </p>
                  </div>
                  <div className="bg-slate-700/30 rounded-lg p-3">
                    <p className="text-slate-400 text-xs uppercase font-semibold mb-1">Est. Speed</p>
                    <p className="text-slate-200 font-semibold text-sm">
                      {analysis.estimated_speed_mbps.toFixed(2)} MB/s
                    </p>
                  </div>
                  <div className="bg-slate-700/30 rounded-lg p-3">
                    <p className="text-slate-400 text-xs uppercase font-semibold mb-1">Content Type</p>
                    <p className="text-slate-200 text-xs font-mono">
                      {analysis.content_type || 'Undetected'}
                    </p>
                  </div>
                  <div className="bg-slate-700/30 rounded-lg p-3">
                    <p className="text-slate-400 text-xs uppercase font-semibold mb-1">Analysis Time</p>
                    <p className="text-slate-200 font-semibold text-sm">{analysis.analysis_duration_ms} ms</p>
                  </div>
                </div>
              </motion.div>

              {/* Connectivity Card */}
              <motion.div
                className="bg-gradient-to-br from-slate-800/50 to-slate-900/50 border border-slate-700 rounded-xl p-6 backdrop-blur-sm"
                whileHover={{ scale: 1.02 }}
              >
                <h3 className="text-xl font-bold text-slate-100 mb-4">🌐 Connectivity & Speed</h3>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-4">
                  <div className="bg-slate-700/30 rounded-lg p-3">
                    <p className="text-slate-400 text-xs uppercase font-semibold mb-1">Health</p>
                    <p className={`font-semibold text-sm ${getHealthColor(analysis.connection_health)}`}>
                      {analysis.connection_health}
                    </p>
                  </div>
                  <div className="bg-slate-700/30 rounded-lg p-3">
                    <p className="text-slate-400 text-xs uppercase font-semibold mb-1">DNS Latency</p>
                    <p className="text-slate-200 font-semibold text-sm">
                      {analysis.dns_latency_ms ?? 'N/A'} ms
                    </p>
                  </div>
                  <div className="bg-slate-700/30 rounded-lg p-3">
                    <p className="text-slate-400 text-xs uppercase font-semibold mb-1">TCP Latency</p>
                    <p className="text-slate-200 font-semibold text-sm">
                      {analysis.tcp_latency_ms ?? 'N/A'} ms
                    </p>
                  </div>
                  <div className="bg-slate-700/30 rounded-lg p-3">
                    <p className="text-slate-400 text-xs uppercase font-semibold mb-1">Pre-Test Speed</p>
                    <p className="text-slate-200 font-semibold text-sm">
                      {analysis.pre_test_speed_mbps?.toFixed(2) ?? 'N/A'} MB/s
                    </p>
                  </div>
                </div>
              </motion.div>

              {/* Strategy & Recommendations */}
              <motion.div
                className="bg-gradient-to-br from-slate-800/50 to-slate-900/50 border border-slate-700 rounded-xl p-6 backdrop-blur-sm"
                whileHover={{ scale: 1.02 }}
              >
                <h3 className="text-xl font-bold text-slate-100 mb-4">🎯 Optimal Strategy</h3>
                <div className="bg-cyan-500/10 border border-cyan-500/30 rounded-lg p-4 mb-6">
                  <p className="text-cyan-300 font-semibold text-lg">{analysis.optimal_strategy}</p>
                </div>

                {analysis.recommendations.length > 0 && (
                  <div>
                    <h4 className="text-slate-300 font-semibold mb-3">💡 Recommendations:</h4>
                    <div className="space-y-3">
                      {[...analysis.recommendations]
                        .sort((a, b) => a.priority - b.priority)
                        .map((rec, idx) => (
                          <motion.div
                            key={idx}
                            initial={{ opacity: 0, x: -10 }}
                            animate={{ opacity: 1, x: 0 }}
                            transition={{ delay: idx * 0.05 }}
                            className="bg-slate-700/30 rounded-lg p-4 border border-slate-600/50 hover:border-cyan-500/50 transition-colors"
                          >
                            <div className="flex items-start gap-3">
                              <div className={`mt-1 px-2 py-0.5 rounded text-xs font-bold uppercase ${
                                rec.category === 'mirror' ? 'bg-red-500/30 text-red-300' :
                                rec.category === 'concurrency' ? 'bg-blue-500/30 text-blue-300' :
                                rec.category === 'retry' ? 'bg-orange-500/30 text-orange-300' :
                                'bg-purple-500/30 text-purple-300'
                              }`}>
                                {rec.category}
                              </div>
                              <div className="flex-1">
                                <p className="text-slate-200 font-semibold">{rec.suggestion}</p>
                                <p className="text-slate-400 text-sm mt-1">📊 Expected: {rec.expected_benefit}</p>
                              </div>
                            </div>
                          </motion.div>
                        ))}
                    </div>
                  </div>
                )}
              </motion.div>

              {/* Mirrors Card */}
              {analysis.detected_mirrors.length > 0 && (
                <motion.div
                  className="bg-gradient-to-br from-slate-800/50 to-slate-900/50 border border-slate-700 rounded-xl p-6 backdrop-blur-sm"
                  whileHover={{ scale: 1.02 }}
                >
                  <h3 className="text-xl font-bold text-slate-100 mb-4">🌐 Available Mirrors ({analysis.detected_mirrors.length})</h3>
                  <div className="space-y-3">
                    {analysis.detected_mirrors.map((mirror, idx) => (
                      <motion.div
                        key={idx}
                        className="bg-slate-700/30 rounded-lg p-4 border border-slate-600/50 cursor-pointer hover:border-cyan-500/50 transition-colors"
                        onClick={() =>
                          setExpandedMirror(expandedMirror === mirror.host ? null : mirror.host)
                        }
                        whileHover={{ scale: 1.01 }}
                      >
                        <div className="flex items-start justify-between">
                          <div className="flex-1">
                            <p className="text-slate-100 font-semibold">{mirror.host}</p>
                            <p className="text-slate-400 text-sm mt-1">{mirror.protocol.toUpperCase()} • Health: {(mirror.health_score * 100).toFixed(0)}%</p>
                          </div>
                          <div className="text-right">
                            <p className="text-cyan-400 font-semibold text-sm">
                              {(analysis.mirror_avg_speeds[mirror.host] || 0).toFixed(2)} MB/s
                            </p>
                            <p className="text-slate-400 text-xs mt-1">
                              {((analysis.mirror_success_rates[mirror.host] || 0) * 100).toFixed(0)}% success
                            </p>
                          </div>
                        </div>

                        {expandedMirror === mirror.host && (
                          <motion.div
                            initial={{ opacity: 0, height: 0 }}
                            animate={{ opacity: 1, height: 'auto' }}
                            exit={{ opacity: 0, height: 0 }}
                            className="mt-3 pt-3 border-t border-slate-600/50"
                          >
                            <p className="text-slate-400 text-xs font-mono break-all">{mirror.url}</p>
                          </motion.div>
                        )}
                      </motion.div>
                    ))}
                  </div>
                </motion.div>
              )}

              {/* Action Buttons */}
              <motion.div
                className="flex gap-3 justify-end"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                transition={{ delay: 0.3 }}
              >
                <button
                  onClick={() => {
                    navigator.clipboard.writeText(urlInput);
                  }}
                  className="px-4 py-2 bg-slate-700 hover:bg-slate-600 text-slate-200 rounded-lg transition-colors flex items-center gap-2"
                >
                  <Copy className="w-4 h-4" />
                  Copy URL
                </button>
                <button
                  onClick={handleGetSummary}
                  className="px-4 py-2 bg-slate-700 hover:bg-slate-600 text-slate-200 rounded-lg transition-colors flex items-center gap-2"
                >
                  Copy Summary
                </button>
                <button
                  onClick={() => setShowSummary(!showSummary)}
                  className="px-4 py-2 bg-cyan-500/20 hover:bg-cyan-500/30 text-cyan-300 rounded-lg transition-colors flex items-center gap-2 border border-cyan-500/50"
                >
                  View Full Report
                </button>
              </motion.div>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  );
};

export default PreFlightAnalysisDashboard;
