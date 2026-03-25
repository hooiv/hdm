import React from 'react';
import { motion } from 'framer-motion';
import { FileText } from 'lucide-react';

interface DownloadItemIconProps {
    category: {
        icon: string;
        label: string;
        color: string;
        bgColor: string;
    };
}

export const DownloadItemIcon: React.FC<DownloadItemIconProps> = ({ category }) => {
    return (
        <div className={`mr-5 p-3.5 rounded-2xl text-2xl ${category.bgColor} ${category.color} border border-white/5`}>
            <motion.div
                whileHover={{ rotate: [0, -10, 10, 0], scale: 1.15 }}
                transition={{ duration: 0.5 }}
            >
                <FileText size={24} className={category.color} strokeWidth={1.5} />
            </motion.div>
        </div>
    );
};
