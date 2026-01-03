import { useRef, useCallback, useEffect } from 'react';

type MessageHandler = (event: { type: string; data: unknown }) => void;

interface UseWebSocketOptions {
  onMessage: MessageHandler;
  onConnect?: () => void;
  onDisconnect?: () => void;
}

export function useWebSocket(url: string, options: UseWebSocketOptions) {
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimeoutRef = useRef<number>();

  const connect = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) return;

    const ws = new WebSocket(url);

    ws.onopen = () => {
      options.onConnect?.();
    };

    ws.onmessage = (event) => {
      try {
        const message = JSON.parse(event.data);
        options.onMessage(message);
      } catch {
        console.error('Failed to parse message:', event.data);
      }
    };

    ws.onclose = () => {
      options.onDisconnect?.();
      reconnectTimeoutRef.current = window.setTimeout(connect, 2000);
    };

    ws.onerror = () => {
      ws.close();
    };

    wsRef.current = ws;
  }, [url, options]);

  const disconnect = useCallback(() => {
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
    }
    wsRef.current?.close();
    wsRef.current = null;
  }, []);

  const send = useCallback((type: string, data: unknown) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify({ type, data }));
    }
  }, []);

  useEffect(() => {
    connect();
    return disconnect;
  }, [connect, disconnect]);

  return { send, disconnect, reconnect: connect };
}
