import React, { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
    Download,
    X,
    File,
    Link as LinkIcon,
    AlertCircle,
    BookOpen,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useToast } from "../contexts/ToastContext";
import type { DockerImageInfo } from "../types";

interface AddDownloadModalProps {
    isOpen: boolean;
    onClose: () => void;
    onStart: (
        url: string,
        filename: string,
        force?: boolean,
        customHeaders?: Record<string, string>,
    ) => void;
}

export const AddDownloadModal: React.FC<AddDownloadModalProps> = ({
    isOpen,
    onClose,
    onStart,
}) => {
    const [url, setUrl] = useState("");
    const [filename, setFilename] = useState("");
    const [isForceMode, setIsForceMode] = useState(false);
    const [isWarcMode, setIsWarcMode] = useState(false);
    const [isResolving, setIsResolving] = useState(false);
    const [isTestingJa3, setIsTestingJa3] = useState(false);
    const [bibtex, setBibtex] = useState("");

    const [isFetchingDocker, setIsFetchingDocker] = useState(false);
    const [dockerInfo, setDockerInfo] = useState<DockerImageInfo | null>(null);

    const toast = useToast();

    const isDoi = url.trim().startsWith("10.") || url.includes("doi.org/");
    const isDocker =
        url.trim().startsWith("docker pull ") || url.trim().startsWith("docker:");
    const isHttp = url.trim().startsWith("http");

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
            console.error("DOI Error:", e);
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
            console.error("Docker Error:", e);
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
            console.error("JA3 Error:", e);
            toast.error(String(e));
        } finally {
            setIsTestingJa3(false);
        }
    };

    // Auto-extract filename
    React.useEffect(() => {
        if (url && !filename) {
            try {
                const parts = url.split("/");
                const last = parts[parts.length - 1].split("?")[0];
                if (last && last.includes(".")) {
                    setFilename(last);
                }
            } catch (e) {
                /* ignore */
            }
        }
    }, [url]);

    const handleSubmit = (e: React.FormEvent) => {
        e.preventDefault();

        if (dockerInfo) {
            dockerInfo.layers.forEach((layer, idx) => {
                const layerFilename = `docker_${dockerInfo.name.replace("/", "_")}_${dockerInfo.tag}_layer${idx}.tar.gz`;
                onStart(layer.url, layerFilename, false, layer.headers);
            });
            setUrl("");
            setFilename("");
            setDockerInfo(null);
            onClose();
            return;
        }

        if (url && filename) {
            if (isWarcMode) {
                // Background task
                invoke("download_as_warc", {
                    url,
                    savePath: `${filename}${filename.endsWith(".warc") ? "" : ".warc"}`,
                })
                    .then(() =>
                        toast.success(`Successfully archived to WARC: ${filename} `),
                    )
                    .catch((e) => toast.error(`Failed to archive WARC: ${e} `));
            } else {
                onStart(url, filename, isForceMode);
            }
            setUrl("");
            setFilename("");
            setIsForceMode(false);
            setBibtex("");
            onClose();
        }
    };

    return (
        <AnimatePresence>
            {isOpen && (
                <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
                    {/* Backdrop */}
                    <motion.div
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        onClick={onClose}
                        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
                    />

                    {/* Modal */}
                    <motion.div
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
                                onClick={onClose}
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
                                        onChange={(e) => setFilename(e.target.value)}
                                        placeholder="file.zip"
                                        className="w-full bg-slate-800/50 border border-slate-700 rounded-lg py-2.5 pl-10 pr-4 text-slate-200 placeholder-slate-600 focus:outline-none focus:border-violet-500/50 focus:ring-1 focus:ring-violet-500/50 transition-all font-medium text-sm"
                                    />
                                </div>
                            </div>

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

                            <div className="flex gap-3 mt-6 pt-2">
                                <button
                                    type="button"
                                    onClick={onClose}
                                    className="flex-1 py-2.5 rounded-lg border border-slate-700 text-slate-400 font-medium hover:bg-slate-800 transition-all text-sm"
                                >
                                    Cancel
                                </button>
                                <button
                                    type="submit"
                                    className={`flex-1 py-2.5 rounded-lg font-bold text-white shadow-lg transition-all flex items-center justify-center gap-2 text-sm ${isForceMode
                                            ? "bg-gradient-to-r from-amber-600 to-orange-600 shadow-amber-900/20 hover:shadow-amber-900/40"
                                            : "bg-gradient-to-r from-blue-600 to-violet-600 shadow-blue-900/20 hover:shadow-blue-900/40"
                                        }`}
                                >
                                    {isForceMode ? "Force Start" : "Start Download"}
                                </button>
                            </div>

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
