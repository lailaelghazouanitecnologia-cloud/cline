import { Bookmark as BookmarkIcon, X, Plus, FileCode } from 'lucide-react';
import type { Bookmark } from '../../types';

interface BookmarksWidgetProps {
  bookmarks: Bookmark[];
  onSelectBookmark: (bookmark: Bookmark) => void;
  onRemoveBookmark: (id: string) => void;
  onAddBookmark: () => void;
}

export function BookmarksWidget({
  bookmarks,
  onSelectBookmark,
  onRemoveBookmark,
  onAddBookmark,
}: BookmarksWidgetProps) {
  return (
    <div className="widget bookmarks-widget">
      <div className="widget-header">
        <span className="widget-title">Bookmarks</span>
        <div className="widget-actions">
          <button className="widget-action-btn" onClick={onAddBookmark} title="Add bookmark">
            <Plus size={12} />
          </button>
        </div>
      </div>

      <div className="widget-content">
        {bookmarks.length === 0 ? (
          <div className="widget-empty">
            <BookmarkIcon size={16} />
            <span>No bookmarks yet</span>
          </div>
        ) : (
          <div className="bookmarks-list">
            {bookmarks.map(bookmark => (
              <div
                key={bookmark.id}
                className="bookmark-item"
                onClick={() => onSelectBookmark(bookmark)}
              >
                <FileCode size={14} className="bookmark-icon" />
                <div className="bookmark-info">
                  <span className="bookmark-name">{bookmark.name}</span>
                  {bookmark.line && (
                    <span className="bookmark-line">:{bookmark.line}</span>
                  )}
                </div>
                <button
                  className="bookmark-remove"
                  onClick={(e) => {
                    e.stopPropagation();
                    onRemoveBookmark(bookmark.id);
                  }}
                >
                  <X size={12} />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
