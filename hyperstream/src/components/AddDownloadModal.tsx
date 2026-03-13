import React, { useState, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
    Download,
    X,
    File,
    Link as LinkIcon,
    AlertCircle,
    BookOpen,
    Plus,
    Trash2,
    Globe,
    Zap,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useToast } from "../contexts/ToastContext";
import { error as logError } from "../utils/logger";
import type { DockerImageInfo, HlsStream, DashManifest } from "../types";

interface AddDownloadModalProps {
    isOpen: boolean;
    onClose: () => void;
    onStart: (
        url: string,
        filename: string,
        force?: boolean,
        customHeaders?: Record<string, string>,
        mirrors?: [string, string][],
        expectedChecksum?: string,
    ) => void;
    initialUrl?: string;
}

export const AddDownloadModal: React.FC<AddDownloadModalProps> = ({
    isOpen,
    onClose,
    onStart,
    initialUrl,
}) => {
    const [url, setUrl] = useState(initialUrl || "");
    const [filename, setFilename] = useState("");
    const [expectedChecksum, setExpectedChecksum] = useState("");
    const userEditedFilename = React.useRef(false);
    const [isForceMode, setIsForceMode] = useState(false);
    const [isWarcMode, setIsWarcMode] = useState(false);
    const [isResolving, setIsResolving] = useState(false);
    const [isTestingJa3, setIsTestingJa3] = useState(false);
    const [bibtex, setBibtex] = useState("");

    const [isFetchingDocker, setIsFetchingDocker] = useState(false);
    const [dockerInfo, setDockerInfo] = useState<DockerImageInfo | null>(null);

    const [hlsInfo, setHlsInfo] = useState<HlsStream | null>(null);
    const [selectedVariantUrl, setSelectedVariantUrl] = useState<string | null>(null);
    const [isParsingHls, setIsParsingHls] = useState(false);

    const [dashManifest, setDashManifest] = useState<DashManifest | null>(null);
    const [isParsingDash, setIsParsingDash] = useState(false);
    const [selectedVideoRepId, setSelectedVideoRepId] = useState<string | null>(null);
    const [selectedAudioRepId, setSelectedAudioRepId] = useState<string | null>(null);

    // Mirror URLs: array of [url, label] pairs
    const [mirrors, setMirrors] = useState<{ url: string; label: string }[]>([]);
    const [showMirrors, setShowMirrors] = useState(false);
    const [isProbing, setIsProbing] = useState(false);
    const [probeResults, setProbeResults] = useState<{ url: string; source: string; latency_ms: number; supports_range: boolean; avg_speed_bps: number; disabled: boolean }[] | null>(null);

    // CAS duplicate detection state
    const [duplicatePath, setDuplicatePath] = useState<string | null>(null);
    const [isCheckingCas, setIsCheckingCas] = useState(false);

    const toast = useToast();

    // Update URL when initialUrl changes (e.g., from drag-and-drop)
    useEffect(() => {
        if (isOpen && initialUrl) {
            setUrl(initialUrl);
            userEditedFilename.current = false;
        }
    }, [initialUrl, isOpen]);

    const resetForm = React.useCallback(() => {
        setUrl(initialUrl || "");
        setFilename("");
        setExpectedChecksum("");
        userEditedFilename.current = false;
        setIsForceMode(false);
        setIsWarcMode(false);
        setBibtex("");
        setDockerInfo(null);
        setHlsInfo(null);
        setSelectedVariantUrl(null);
        setDashManifest(null);
        setIsParsingDash(false);
        setSelectedVideoRepId(null);
        setSelectedAudioRepId(null);
        setMirrors([]);
        setShowMirrors(false);
        setProbeResults(null);
        setDuplicatePath(null);
        setIsCheckingCas(false);
    }, [initialUrl]);

    const handleClose = React.useCallback(() => {
        resetForm();
        onClose();
    }, [onClose, resetForm]);

    const isDoi = url.trim().startsWith("10.") || url.includes("doi.org/");
    const isDocker =
        url.trim().startsWith("docker pull ") || url.trim().startsWith("docker:");
    const isHttp = url.trim().startsWith("http");
    const isHls = url.trim().toLowerCase().endsWith(".m3u8");
    const isDash = isHttp && url.trim().toLowerCase().includes(".mpd");

    /** Basic URL validation — must be http(s), DOI, or Docker */
    const isValidUrl = React.useMemo(() => {
        const trimmed = url.trim();
        if (!trimmed) return true; // empty is not "invalid", just not ready
        return /^https?:\/\/.+/.test(trimmed) ||
               trimmed.startsWith("10.") || trimmed.includes("doi.org/") ||
               trimmed.startsWith("docker pull ") || trimmed.startsWith("docker:");
    }, [url]);

    const canSubmit = url.trim() && filename.trim() && isValidUrl;

    const handleResolveDoi = async () => {
        if (!url) return;
        setIsResolving(true);
        setBibtex("");
        try {
            const result = await invoke<string>("resolve_doi", { doi: url });
            setBibtex(result);

            // Try to extract title for filename
            const titleMatch = result.match(/title\s*=\s*[{"]([^}"]+)[}"]/i);
            if (titleMatch && titleMatch[1]) {
                const safeTitle = titleMatch[1]
                    .replace(/[^a-z0-9]/gi, "_")
                    .toLowerCase();
                setFilename(`${safeTitle}.pdf`);
            }
        } catch (e) {
            logError("DOI Error:", e);
            toast.error(`Failed to resolve DOI: ${e}`);
        } finally {
            setIsResolving(false);
        }
    };

    const handleFetchDocker = async () => {
        if (!url) return;
        setIsFetchingDocker(true);
        setDockerInfo(null);
        try {
            let image = url.trim();
            if (image.startsWith("docker pull ")) {
                image = image.replace("docker pull ", "").trim();
            } else if (image.startsWith("docker:")) {
                image = image.replace("docker:", "").trim();
            }

            const info = await invoke<DockerImageInfo>("fetch_docker_manifest", { image });
            setDockerInfo(info);
            setFilename(`${info.name.replace("/", "_")}_${info.tag}.tar`);
        } catch (e) {
            logError("Docker Error:", e);
            toast.error("Docker pull failed: " + e);
        } finally {
            setIsFetchingDocker(false);
        }
    };

    const handleTestJa3 = async () => {
        if (!url) return;
        setIsTestingJa3(true);
        try {
            const response = await invoke<string>("fetch_with_ja3", { url, browser: "chrome" });
            toast.success("JA3 Spoof Success. Server responded with: " + response.substring(0, 100) + "...");
        } catch (e) {
            logError("JA3 Error:", e);
            toast.error(String(e));
        } finally {
            setIsTestingJa3(false);
        }
    };

    const addMirror = () => {
        setMirrors(prev => [...prev, { url: '', label: `Mirror ${prev.length + 1}` }]);
        setShowMirrors(true);
    };

    const removeMirror = (idx: number) => {
        setMirrors(prev => prev.filter((_, i) => i !== idx));
        setProbeResults(null);
    };

    const updateMirror = (idx: number, field: 'url' | 'label', value: string) => {
        setMirrors(prev => prev.map((m, i) => i === idx ? { ...m, [field]: value } : m));
    };

    const handleProbe = async () => {
        const validMirrors = mirrors.filter(m => m.url.trim());
        if (validMirrors.length === 0 && !url.trim()) return;
        setIsProbing(true);
        setProbeResults(null);
        try {
            const mirrorPairs: [string, string][] = validMirrors.map(m => [m.url.trim(), m.label || 'Mirror']);
            const results = await invoke<{ url: string; source: string; latency_ms: number; supports_range: boolean; avg_speed_bps: number; disabled: boolean }[]>("probe_mirrors", {
                primaryUrl: url.trim(),
                mirrorUrls: mirrorPairs,
            });
            setProbeResults(results);
        } catch (e) {
            logError("Probe Error:", e);
            toast.error("Mirror probe failed: " + e);
        } finally {
            setIsProbing(false);
        }
    };

    // Auto-extract filename from URL (re-triggers on URL change unless user manually edited)
    React.useEffect(() => {
        if (!url) return;
        if (userEditedFilename.current) return;
        try {
            const parts = url.split("/");
            const last = parts[parts.length - 1].split("?")[0];
            if (last && last.includes(".")) {
                setFilename(last);
            }
        } catch (e) {
            /* ignore */
        }
    }, [url]);

    // CAS duplicate detection: check ETag/MD5 from HEAD request against local CAS DB
    React.useEffect(() => {
        if (!url || !isHttp) {
            setDuplicatePath(null);
            return;
        }
        const trimmed = url.trim();
        if (!/^https?:\/\/.+/.test(trimmed)) return;

        let cancelled = false;
        setIsCheckingCas(true);
        setDuplicatePath(null);

        invoke<{ etag: string | null; content_md5: string | null }>('head_url_metadata', { url: trimmed })
            .then((meta) => {
                if (cancelled) return;
                if (!meta.etag && !meta.content_md5) {
                    setIsCheckingCas(false);
                    return;
                }
                return invoke<string | null>('check_cas_duplicate', {
                    etag: meta.etag,
                    md5: meta.content_md5,
                });
            })
            .then((path) => {
                if (cancelled) return;
                setDuplicatePath(path ?? null);
            })
            .catch(() => {
                // HEAD request may fail, that's OK — silently skip CAS check
            })
            .finally(() => {
                if (!cancelled) setIsCheckingCas(false);
            });

        return () => { cancelled = true; };
    }, [url, isHttp]);

    // whenever URL looks like HLS manifest we parse it to offer variants
    React.useEffect(() => {
        if (!isHls) {
            setHlsInfo(null);
            setSelectedVariantUrl(null);
            return;
        }
        setIsParsingHls(true);
        invoke<HlsStream>("parse_hls_stream", { url })
            .then((info) => {
                setHlsInfo(info);
                if (info.is_master && info.variants.length > 0) {
                    setSelectedVariantUrl(info.variants[0].url);
                } else {
                    setSelectedVariantUrl(url);
                }
            })
            .catch((e) => {
                logError("HLS parse failed", e);
                setHlsInfo(null);
                setSelectedVariantUrl(null);
            })
            .finally(() => setIsParsingHls(false));
    }, [url, isHls]);

    // whenever URL looks like DASH manifest we parse it to offer representations
    React.useEffect(() => {
        if (!isDash) {
            setDashManifest(null);
            setSelectedVideoRepId(null);
            setSelectedAudioRepId(null);
            return;
        }
        setIsParsingDash(true);
        invoke<DashManifest>("fetch_dash_manifest", { url })
            .then((manifest) => {
                setDashManifest(manifest);
                if (manifest.video_representations.length > 0) {
                    setSelectedVideoRepId(manifest.video_representations[0].id);
                }
                if (manifest.audio_representations.length > 0) {
                    setSelectedAudioRepId(manifest.audio_representations[0].id);
                }
            })
            .catch((e) => {
                logError("DASH parse failed", e);
                setDashManifest(null);
                setSelectedVideoRepId(null);
                setSelectedAudioRepId(null);
            })
            .finally(() => setIsParsingDash(false));
    }, [url, isDash]);

    const handleSubmit = (e: React.FormEvent) => {
        e.preventDefault();
        if (!canSubmit) return;
        const normalizedChecksum = expectedChecksum.trim() || undefined;

        if (dockerInfo) {
            dockerInfo.layers.forEach((layer, idx) => {
                const layerFilename = `docker_${dockerInfo.name.replace("/", "_")}_${dockerInfo.tag}_layer${idx}.tar.gz`;
                onStart(layer.url, layerFilename, false, layer.headers);
            });
            handleClose();
            return;
        }

        if (url && filename) {
            if (isWarcMode) {
                // Sanitize WARC filename: strip path separators, illegal chars, reserved names
                let safeWarcName = filename.replace(/[/\\]/g, '_').replace(/[<>:"|?*\x00-\x1f]/g, '_').replace(/[\s.]+$/, '');
                if (/^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$/i.test(safeWarcName.replace(/\.[^.]*$/, ''))) {
                    safeWarcName = '_' + safeWarcName;
                }
                if (!safeWarcName) safeWarcName = 'archive';
                // Background task
                invoke("download_as_warc", {
                    url,
                    savePath: `${safeWarcName}${safeWarcName.endsWith(".warc") ? "" : ".warc"}`,
                })
                    .then(() =>
                        toast.success(`Successfully archived to WARC: ${filename} `),
                    )
                    .catch((e) => toast.error(`Failed to archive WARC: ${e} `));
            } else {
                const validMirrors: [string, string][] = mirrors
                    .filter(m => m.url.trim())
                    .map(m => [m.url.trim(), m.label || 'Mirror']);

                let customHeaders: Record<string, string> | undefined = undefined;
                if (isDash) {
                    customHeaders = {};
                    if (selectedVideoRepId) customHeaders["X-Dash-Video-Rep"] = selectedVideoRepId;
                    if (selectedAudioRepId) customHeaders["X-Dash-Audio-Rep"] = selectedAudioRepId;
                    if (Object.keys(customHeaders).length === 0) customHeaders = undefined;
                }

                if (isHls && selectedVariantUrl) {
                    onStart(selectedVariantUrl, filename, isForceMode, customHeaders, validMirrors.length > 0 ? validMirrors : undefined, normalizedChecksum);
                } else {
                    onStart(url, filename, isForceMode, customHeaders, validMirrors.length > 0 ? validMirrors : undefined, normalizedChecksum);
                }
            }
            handleClose();
        }
    };

    // Close on Escape key
    useEffect(() => {
        if (!isOpen) return;
        const onKey = (e: KeyboardEvent) => {
            if (e.key === 'Escape') {
                e.preventDefault();
                handleClose();
            }
        };
        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    }, [handleClose, isOpen]);

    return (
        <AnimatePresence>
            {isOpen && (
                <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
                    {/* Backdrop */}
                    <motion.div
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        onClick={handleClose}
                        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
                    />

                    {/* Modal */}
                    <motion.div
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="add-download-title"
                        initial={{ scale: 0.95, opacity: 0, y: 10 }}
                        animate={{ scale: 1, opacity: 1, y: 0 }}
                        exit={{ scale: 0.95, opacity: 0, y: 10 }}
                        className="relative w-full max-w-md bg-slate-900/90 border border-slate-700/50 rounded-xl shadow-2xl p-6 overflow-hidden"
                    >
                        <div className="absolute top-0 left-0 w-full h-1 bg-gradient-to-r from-blue-500 to-violet-500" />

                        <div className="flex justify-between items-center mb-6">
                            <h2 className="text-xl font-bold bg-gradient-to-r from-white to-slate-400 bg-clip-text text-transparent flex items-center gap-2">
                                <Download className="text-blue-500" size={24} />
                                Add Download
                            </h2>
                            <button
                                onClick={handleClose}
                                className="text-slate-400 hover:text-white transition-colors"
                            >
                                <X size={20} />
                            </button>
                        </div>

                        <form onSubmit={handleSubmit} className="space-y-4">
                            <div className="space-y-1">
                                <label className="text-xs uppercase font-semibold text-slate-500 tracking-wider ml-1">
                                    Download URL
                                </label>
                                <div className="relative group">
                                    <LinkIcon
                                        className="absolute left-3 top-3 text-slate-500 group-focus-within:text-blue-500 transition-colors"
                                        size={18}
                                    />
                                    <input
                                        type="text"
                                        value={url}
                                        onChange={(e) => setUrl(e.target.value)}
                                        placeholder="https://example.com/file.zip or 10.1000/xyz123"
                                        autoFocus
                                        className="w-full bg-slate-800/50 border border-slate-700 rounded-lg py-2.5 pl-10 pr-4 text-slate-200 placeholder-slate-600 focus:outline-none focus:border-blue-500/50 focus:ring-1 focus:ring-blue-500/50 transition-all font-mono text-sm"
                                    />
                                    {isDoi && (
                                        <button
                                            type="button"
                                            onClick={handleResolveDoi}
                                            disabled={isResolving}
                                            className="absolute right-2 top-2 bottom-2 px-3 bg-blue-500/20 text-blue-400 hover:bg-blue-500/30 rounded flex items-center gap-1 text-xs font-semibold transition-colors disabled:opacity-50"
                                        >
                                            <BookOpen size={14} />
                                            {isResolving ? "Resolving..." : "Resolve DOI"}
                                        </button>
                                    )}
                                    {isDocker && !dockerInfo && (
                                        <button
                                            type="button"
                                            onClick={handleFetchDocker}
                                            disabled={isFetchingDocker}
                                            className="absolute right-2 top-2 bottom-2 px-3 bg-cyan-500/20 text-cyan-400 hover:bg-cyan-500/30 rounded flex items-center gap-1 text-xs font-semibold transition-colors disabled:opacity-50"
                                        >
                                            <AlertCircle size={14} />
                                            {isFetchingDocker ? "Fetching..." : "Fetch Manifest"}
                                        </button>
                                    )}
                                    {isHttp && !isDoi && !isDocker && (
                                        <button
                                            type="button"
                                            onClick={handleTestJa3}
                                            disabled={isTestingJa3}
                                            className="absolute right-2 top-2 bottom-2 px-3 bg-fuchsia-500/20 text-fuchsia-400 hover:bg-fuchsia-500/30 rounded flex items-center gap-1 text-xs font-semibold transition-colors disabled:opacity-50"
                                        >
                                            <AlertCircle size={14} />
                                            {isTestingJa3 ? "Testing..." : "Test JA3"}
                                        </button>
                                    )}
                                </div>
                            </div>

                            {isHls && (
                                <div className="space-y-1">
                                    <label className="text-xs uppercase font-semibold text-slate-500 tracking-wider ml-1">
                                        HLS Variant
                                    </label>
                                    <div className="relative group">
                                        {isParsingHls ? (
                                            <div className="text-sm text-slate-400 italic">Parsing playlist...</div>
                                        ) : hlsInfo ? (
                                            hlsInfo.is_master ? (
                                                <select
                                                    value={selectedVariantUrl || ''}
                                                    onChange={e => setSelectedVariantUrl(e.target.value)}
                                                    className="w-full bg-slate-800/50 border border-slate-700 rounded-lg py-2.5 pl-3 pr-4 text-slate-200 placeholder-slate-600 focus:outline-none focus:border-blue-500/50 focus:ring-1 focus:ring-blue-500/50 transition-all font-mono text-sm"
                                                >
                                                    {hlsInfo.variants.map(v => (
                                                        <option key={v.url} value={v.url}>
                                                            {v.resolution || `${(v.bandwidth/1000).toFixed(0)}kbps`}
                                                        </option>
                                                    ))}
                                                </select>
                                            ) : (
                                                <div className="text-sm text-slate-400">
                                                    Media playlist – {hlsInfo.segments.length} segments
                                                </div>
                                            )
                                        ) : (
                                            <div className="text-sm text-red-400">Failed to parse HLS stream</div>
                                        )}
                                    </div>
                                </div>
                            )}

                            {isDash && (
                                <div className="space-y-3">
                                    <label className="text-xs uppercase font-semibold text-slate-500 tracking-wider ml-1">
                                        DASH Characteristics
                                    </label>
                                    <div className="relative group p-3 bg-slate-800/30 rounded-lg border border-slate-700/50">
                                        {isParsingDash ? (
                                            <div className="text-sm text-slate-400 italic">Probing MPD Manifest...</div>
                                        ) : dashManifest ? (
                                            <div className="flex flex-col gap-3">
                                                {dashManifest.video_representations.length > 0 && (
                                                    <div>
                                                        <label className="text-[11px] uppercase font-bold text-slate-400 block mb-1">Video Track</label>
                                                        <select
                                                            value={selectedVideoRepId || ""}
                                                            onChange={(e) => setSelectedVideoRepId(e.target.value)}
                                                            className="w-full bg-slate-800 border border-slate-600 rounded p-2 text-slate-200 text-sm focus:outline-none focus:border-blue-500 transition-colors"
                                                        >
                                                            {dashManifest.video_representations.map(v => (
                                                                <option key={v.id} value={v.id}>
                                                                    ID: {v.id} • {v.width ? `${v.width}x${v.height} • ` : ""}{Math.round(v.bandwidth / 1000)} kbps {v.codecs ? `(${v.codecs})` : ""}
                                                                </option>
                                                            ))}
                                                        </select>
                                                    </div>
                                                )}
                                                {dashManifest.audio_representations.length > 0 && (
                                                    <div>
                                                        <label className="text-[11px] uppercase font-bold text-slate-400 block mb-1">Audio Track</label>
                                                        <select
                                                            value={selectedAudioRepId || ""}
                                                            onChange={(e) => setSelectedAudioRepId(e.target.value)}
                                                            className="w-full bg-slate-800 border border-slate-600 rounded p-2 text-slate-200 text-sm focus:outline-none focus:border-blue-500 transition-colors"
                                                        >
                                                            {dashManifest.audio_representations.map(a => (
                                                                <option key={a.id} value={a.id}>
                                                                    ID: {a.id} • {Math.round(a.bandwidth / 1000)} kbps {a.codecs ? `(${a.codecs})` : ""}
                                                                </option>
                                                            ))}
                                                        </select>
                                                    </div>
                                                )}
                                                <div className="text-xs text-slate-500 mt-1">
                                                    Video & Audio tracks will be muxed automatically.
                                                </div>
                                            </div>
                                        ) : (
                                            <div className="text-sm text-amber-500 italic">Could not probe DASH MPD</div>
                                        )}
                                    </div>
                                </div>
                            )}

                            <div className="space-y-1">
                                <label className="text-xs uppercase font-semibold text-slate-500 tracking-wider ml-1">
                                    Filename
                                </label>
                                <div className="relative group">
                                    <File
                                        className="absolute left-3 top-3 text-slate-500 group-focus-within:text-violet-500 transition-colors"
                                        size={18}
                                    />
                                    <input
                                        type="text"
                                        value={filename}
                                        onChange={(e) => {
                                            userEditedFilename.current = true;
                                            setFilename(e.target.value);
                                        }}
                                        placeholder="file.zip"
                                        className="w-full bg-slate-800/50 border border-slate-700 rounded-lg py-2.5 pl-10 pr-4 text-slate-200 placeholder-slate-600 focus:outline-none focus:border-violet-500/50 focus:ring-1 focus:ring-violet-500/50 transition-all font-medium text-sm"
                                    />
                                </div>
                            </div>

                            {!dockerInfo && !isWarcMode && (
                                <div className="space-y-1">
                                    <label className="text-xs uppercase font-semibold text-slate-500 tracking-wider ml-1">
                                        Expected Checksum (optional)
                                    </label>
                                    <input
                                        type="text"
                                        value={expectedChecksum}
                                        onChange={(e) => setExpectedChecksum(e.target.value)}
                                        placeholder="sha256:abc123... or md5:..."
                                        className="w-full bg-slate-800/50 border border-slate-700 rounded-lg py-2.5 px-3 text-slate-200 placeholder-slate-600 focus:outline-none focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/50 transition-all font-mono text-sm"
                                    />
                                    <p className="text-[11px] text-slate-500 px-1">
                                        Supports <code>sha256:</code>, <code>md5:</code>, <code>crc32:</code>, or raw hex checksums.
                                    </p>
                                </div>
                            )}

                            {bibtex && (
                                <motion.div
                                    initial={{ opacity: 0, height: 0 }}
                                    animate={{ opacity: 1, height: "auto" }}
                                    className="bg-slate-900/50 border border-slate-700/50 rounded-lg p-3 overflow-hidden text-xs text-slate-400 font-mono"
                                >
                                    <pre className="whitespace-pre-wrap max-h-32 overflow-y-auto custom-scrollbar">
                                        {bibtex}
                                    </pre>
                                </motion.div>
                            )}

                            {/* CAS Duplicate Detection Warning */}
                            {duplicatePath && (
                                <motion.div
                                    initial={{ opacity: 0, height: 0 }}
                                    animate={{ opacity: 1, height: "auto" }}
                                    className="bg-amber-900/20 border border-amber-600/40 rounded-lg p-3 flex items-start gap-3"
                                >
                                    <AlertCircle size={18} className="text-amber-400 flex-shrink-0 mt-0.5" />
                                    <div className="flex-1 min-w-0">
                                        <p className="text-sm font-bold text-amber-300">Duplicate File Detected</p>
                                        <p className="text-xs text-amber-400/80 mt-1">You already have this file at:</p>
                                        <p className="text-xs text-slate-300 font-mono mt-1 truncate" title={duplicatePath}>{duplicatePath}</p>
                                        <p className="text-[10px] text-amber-500/70 mt-2">You can still download again if needed.</p>
                                    </div>
                                </motion.div>
                            )}
                            {isCheckingCas && (
                                <div className="flex items-center gap-2 text-xs text-slate-500">
                                    <div className="w-3 h-3 border border-slate-500 border-t-transparent rounded-full animate-spin" />
                                    Checking for duplicates...
                                </div>
                            )}

                            {dockerInfo && (
                                <motion.div
                                    initial={{ opacity: 0, y: 10 }}
                                    animate={{ opacity: 1, y: 0 }}
                                    className="bg-cyan-900/20 border border-cyan-700/50 rounded-lg p-4 text-cyan-100 flex flex-col gap-2"
                                >
                                    <div className="flex items-center gap-2 text-cyan-400 font-bold mb-1">
                                        <Download size={18} />
                                        Docker Image Context
                                    </div>
                                    <p className="text-sm font-mono">
                                        <span className="text-cyan-500">Image:</span>{" "}
                                        {dockerInfo.name}:{dockerInfo.tag}
                                    </p>
                                    <p className="text-sm">
                                        <span className="text-cyan-500 font-mono">Layers:</span>{" "}
                                        {dockerInfo.layers?.length || 0} manifest blobs
                                    </p>
                                    <p className="text-xs text-cyan-400/80 mt-2">
                                        Submitting will enqueue all {dockerInfo.layers?.length || 0}{" "}
                                        layers as rapid parallel downloads.
                                    </p>
                                </motion.div>
                            )}

                            {!dockerInfo && (
                                <>
                                    {/* Force Download Toggle (Shift Key visualizer) */}
                                    <div
                                        className={`flex items-center gap-3 p-3 rounded-lg border transition-all cursor-pointer ${isForceMode ? "bg-amber-900/20 border-amber-500/30" : "bg-slate-800/30 border-transparent hover:bg-slate-800/50"}`}
                                        onClick={() => setIsForceMode(!isForceMode)}
                                    >
                                        <div
                                            className={`w-4 h-4 rounded border flex items-center justify-center transition-all ${isForceMode ? "bg-amber-500 border-amber-500" : "border-slate-600"}`}
                                        >
                                            {isForceMode && (
                                                <div className="w-2 h-2 bg-white rounded-sm" />
                                            )}
                                        </div>
                                        <div className="flex-1">
                                            <p
                                                className={`text-sm font-medium ${isForceMode ? "text-amber-400" : "text-slate-400"}`}
                                            >
                                                Force Download Mode
                                            </p>
                                            <p className="text-xs text-slate-500">
                                                Bypasses pre-checks. Use for problematic links.
                                            </p>
                                        </div>
                                        {isForceMode && (
                                            <AlertCircle size={16} className="text-amber-500" />
                                        )}
                                    </div>

                                    {/* WARC Mode Toggle */}
                                    <div
                                        className={`flex items-center gap-3 p-3 rounded-lg border transition-all cursor-pointer ${isWarcMode ? "bg-indigo-900/20 border-indigo-500/30" : "bg-slate-800/30 border-transparent hover:bg-slate-800/50"}`}
                                        onClick={() => {
                                            setIsWarcMode(!isWarcMode);
                                            setIsForceMode(false);
                                        }}
                                    >
                                        <div
                                            className={`w-4 h-4 rounded border flex items-center justify-center transition-all ${isWarcMode ? "bg-indigo-500 border-indigo-500" : "border-slate-600"}`}
                                        >
                                            {isWarcMode && (
                                                <div className="w-2 h-2 bg-white rounded-sm" />
                                            )}
                                        </div>
                                        <div className="flex-1">
                                            <p
                                                className={`text-sm font-medium ${isWarcMode ? "text-indigo-400" : "text-slate-400"}`}
                                            >
                                                Save as WARC Archive
                                            </p>
                                            <p className="text-xs text-slate-500">
                                                Downloads entire page and assets into a .warc file.
                                            </p>
                                        </div>
                                    </div>
                                </>
                            )}

                            {/* Multi-Source / Mirror URLs */}
                            {!dockerInfo && !isWarcMode && isHttp && (
                                <div className="space-y-2">
                                    <button
                                        type="button"
                                        onClick={() => { setShowMirrors(!showMirrors); if (!showMirrors && mirrors.length === 0) addMirror(); }}
                                        className={`flex items-center gap-2 w-full p-2.5 rounded-lg border transition-all text-sm ${showMirrors ? 'bg-emerald-900/20 border-emerald-500/30 text-emerald-400' : 'bg-slate-800/30 border-transparent hover:bg-slate-800/50 text-slate-400'}`}
                                    >
                                        <Globe size={16} />
                                        <span className="font-medium">Multi-Source / Mirrors</span>
                                        {mirrors.filter(m => m.url.trim()).length > 0 && (
                                            <span className="ml-auto bg-emerald-500/20 text-emerald-400 text-xs font-bold px-2 py-0.5 rounded-full">
                                                {mirrors.filter(m => m.url.trim()).length}
                                            </span>
                                        )}
                                    </button>

                                    <AnimatePresence>
                                        {showMirrors && (
                                            <motion.div
                                                initial={{ opacity: 0, height: 0 }}
                                                animate={{ opacity: 1, height: 'auto' }}
                                                exit={{ opacity: 0, height: 0 }}
                                                className="space-y-2 overflow-hidden"
                                            >
                                                <p className="text-xs text-slate-500 px-1">
                                                    Add mirror URLs to download from multiple sources simultaneously for faster speeds.
                                                </p>
                                                {mirrors.map((mirror, idx) => (
                                                    <div key={idx} className="flex gap-2 items-center">
                                                        <input
                                                            type="text"
                                                            value={mirror.url}
                                                            onChange={e => updateMirror(idx, 'url', e.target.value)}
                                                            placeholder="https://mirror.example.com/file.zip"
                                                            className="flex-1 bg-slate-800/50 border border-slate-700 rounded-lg py-2 px-3 text-slate-200 placeholder-slate-600 focus:outline-none focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/50 transition-all font-mono text-xs"
                                                        />
                                                        <input
                                                            type="text"
                                                            value={mirror.label}
                                                            onChange={e => updateMirror(idx, 'label', e.target.value)}
                                                            placeholder="Label"
                                                            className="w-24 bg-slate-800/50 border border-slate-700 rounded-lg py-2 px-2 text-slate-300 placeholder-slate-600 focus:outline-none focus:border-emerald-500/50 text-xs"
                                                        />
                                                        <button
                                                            type="button"
                                                            onClick={() => removeMirror(idx)}
                                                            className="text-slate-500 hover:text-red-400 transition-colors p-1"
                                                        >
                                                            <Trash2 size={14} />
                                                        </button>
                                                    </div>
                                                ))}
                                                <div className="flex gap-2">
                                                    <button
                                                        type="button"
                                                        onClick={addMirror}
                                                        className="flex items-center gap-1 text-xs text-emerald-400 hover:text-emerald-300 transition-colors px-2 py-1 rounded bg-emerald-500/10 hover:bg-emerald-500/20"
                                                    >
                                                        <Plus size={12} /> Add Mirror
                                                    </button>
                                                    {mirrors.some(m => m.url.trim()) && (
                                                        <button
                                                            type="button"
                                                            onClick={handleProbe}
                                                            disabled={isProbing}
                                                            className="flex items-center gap-1 text-xs text-blue-400 hover:text-blue-300 transition-colors px-2 py-1 rounded bg-blue-500/10 hover:bg-blue-500/20 disabled:opacity-50"
                                                        >
                                                            <Zap size={12} /> {isProbing ? 'Probing...' : 'Test Mirrors'}
                                                        </button>
                                                    )}
                                                </div>
                                                {probeResults && (
                                                    <motion.div
                                                        initial={{ opacity: 0 }}
                                                        animate={{ opacity: 1 }}
                                                        className="bg-slate-800/50 rounded-lg p-2 space-y-1 text-xs"
                                                    >
                                                        {probeResults.map((r, i) => (
                                                            <div key={i} className={`flex items-center justify-between px-2 py-1 rounded ${r.disabled ? 'text-red-400/60' : 'text-slate-300'}`}>
                                                                <span className="truncate flex-1 font-mono">{r.source}</span>
                                                                <span className={`mx-2 ${r.supports_range ? 'text-emerald-400' : 'text-amber-400'}`}>
                                                                    {r.supports_range ? 'Range ✓' : 'No Range'}
                                                                </span>
                                                                <span className="text-slate-400 w-16 text-right">
                                                                    {r.latency_ms < 999999 ? `${r.latency_ms}ms` : 'Timeout'}
                                                                </span>
                                                            </div>
                                                        ))}
                                                    </motion.div>
                                                )}
                                            </motion.div>
                                        )}
                                    </AnimatePresence>
                                </div>
                            )}

                            <div className="flex gap-3 mt-6 pt-2">
                                <button
                                    type="button"
                                    onClick={handleClose}
                                    className="flex-1 py-2.5 rounded-lg border border-slate-700 text-slate-400 font-medium hover:bg-slate-800 transition-all text-sm"
                                >
                                    Cancel
                                </button>
                                <button
                                    type="submit"
                                    disabled={!canSubmit}
                                    className={`flex-1 py-2.5 rounded-lg font-bold text-white shadow-lg transition-all flex items-center justify-center gap-2 text-sm ${!canSubmit
                                            ? "opacity-50 cursor-not-allowed bg-slate-700"
                                            : isForceMode
                                                ? "bg-gradient-to-r from-amber-600 to-orange-600 shadow-amber-900/20 hover:shadow-amber-900/40"
                                                : mirrors.some(m => m.url.trim())
                                                    ? "bg-gradient-to-r from-emerald-600 to-cyan-600 shadow-emerald-900/20 hover:shadow-emerald-900/40"
                                                    : "bg-gradient-to-r from-blue-600 to-violet-600 shadow-blue-900/20 hover:shadow-blue-900/40"
                                        }`}
                                >
                                    {isForceMode ? "Force Start" : mirrors.some(m => m.url.trim()) ? "Multi-Source Start" : "Start Download"}
                                </button>
                            </div>

                            {!isValidUrl && url.trim() && (
                                <p className="text-center text-xs text-red-400 mt-1">
                                    URL must start with http:// or https:// (or be a DOI/Docker reference)
                                </p>
                            )}

                            <p className="text-center text-xs text-slate-600">
                                Tip: Hold{" "}
                                <kbd className="font-mono bg-slate-800 px-1 rounded text-slate-500">
                                    Shift
                                </kbd>{" "}
                                while clicking Start to force.
                            </p>
                        </form>
                    </motion.div>
                </div>
            )}
        </AnimatePresence>
    );
};
