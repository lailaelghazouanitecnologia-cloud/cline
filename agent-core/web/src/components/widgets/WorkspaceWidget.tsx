import { useState } from 'react';
import {
  ChevronRight,
  ChevronDown,
  Folder,
  FolderOpen,
  File,
  FileCode,
  FileJson,
  Plus,
  RefreshCw
} from 'lucide-react';
import type { Workspace, FileNode } from '../../types';

interface WorkspaceWidgetProps {
  workspaces: Workspace[];
  activeWorkspace: Workspace | null;
  fileTree: FileNode[];
  onSelectWorkspace: (id: string) => void;
  onAddWorkspace: () => void;
  onRefresh: () => void;
  onSelectFile: (path: string) => void;
}

function getFileIcon(name: string) {
  const ext = name.split('.').pop()?.toLowerCase();
  if (['ts', 'tsx', 'js', 'jsx', 'rs', 'py'].includes(ext || '')) {
    return <FileCode size={14} />;
  }
  if (['json', 'toml', 'yaml', 'yml'].includes(ext || '')) {
    return <FileJson size={14} />;
  }
  return <File size={14} />;
}

function FileTreeItem({
  node,
  depth,
  onToggle,
  onSelect
}: {
  node: FileNode;
  depth: number;
  onToggle: (path: string) => void;
  onSelect: (path: string) => void;
}) {
  const isFolder = node.type === 'folder';
  const paddingLeft = 8 + depth * 12;

  const handleClick = () => {
    if (isFolder) {
      onToggle(node.path);
    } else {
      onSelect(node.path);
    }
  };

  return (
    <>
      <div
        className="tree-item"
        style={{ paddingLeft }}
        onClick={handleClick}
      >
        <span className="tree-icon">
          {isFolder ? (
            node.isExpanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />
          ) : null}
        </span>
        <span className="tree-type-icon">
          {isFolder ? (
            node.isExpanded ? <FolderOpen size={14} /> : <Folder size={14} />
          ) : (
            getFileIcon(node.name)
          )}
        </span>
        <span className="tree-name">{node.name}</span>
      </div>
      {isFolder && node.isExpanded && node.children?.map(child => (
        <FileTreeItem
          key={child.path}
          node={child}
          depth={depth + 1}
          onToggle={onToggle}
          onSelect={onSelect}
        />
      ))}
    </>
  );
}

export function WorkspaceWidget({
  workspaces,
  activeWorkspace,
  fileTree,
  onSelectWorkspace,
  onAddWorkspace,
  onRefresh,
  onSelectFile,
}: WorkspaceWidgetProps) {
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());

  const togglePath = (path: string) => {
    setExpandedPaths(prev => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  };

  const enrichTree = (nodes: FileNode[]): FileNode[] => {
    return nodes.map(node => ({
      ...node,
      isExpanded: expandedPaths.has(node.path),
      children: node.children ? enrichTree(node.children) : undefined,
    }));
  };

  const enrichedTree = enrichTree(fileTree);

  return (
    <div className="widget workspace-widget">
      <div className="widget-header">
        <span className="widget-title">Workspace</span>
        <div className="widget-actions">
          <button className="widget-action-btn" onClick={onRefresh} title="Refresh">
            <RefreshCw size={12} />
          </button>
          <button className="widget-action-btn" onClick={onAddWorkspace} title="Add workspace">
            <Plus size={12} />
          </button>
        </div>
      </div>

      {workspaces.length > 1 && (
        <div className="workspace-selector">
          <select
            value={activeWorkspace?.id || ''}
            onChange={e => onSelectWorkspace(e.target.value)}
            className="workspace-select"
          >
            {workspaces.map(ws => (
              <option key={ws.id} value={ws.id}>{ws.name}</option>
            ))}
          </select>
        </div>
      )}

      <div className="widget-content">
        {activeWorkspace && (
          <div className="workspace-path">{activeWorkspace.path}</div>
        )}
        <div className="file-tree">
          {enrichedTree.map(node => (
            <FileTreeItem
              key={node.path}
              node={node}
              depth={0}
              onToggle={togglePath}
              onSelect={onSelectFile}
            />
          ))}
          {enrichedTree.length === 0 && (
            <div className="tree-empty">No files</div>
          )}
        </div>
      </div>
    </div>
  );
}
