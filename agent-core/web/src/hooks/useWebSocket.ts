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
  const optionsRef = useRef(options);

  useEffect(() => {
    optionsRef.current = options;
  }, [options]);

  const connect = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) return;
    if (wsRef.current?.readyState === WebSocket.CONNECTING) return;

    try {
      const ws = new WebSocket(url);

      ws.onopen = () => {
        optionsRef.current.onConnect?.();
      };

      ws.onmessage = (event) => {
        try {
          const message = JSON.parse(event.data);
          optionsRef.current.onMessage(message);
        } catch {
          console.error('Failed to parse message:', event.data);
        }
      };

      ws.onclose = () => {
        optionsRef.current.onDisconnect?.();
        wsRef.current = null;
        reconnectTimeoutRef.current = window.setTimeout(connect, 3000);
      };

      ws.onerror = (error) => {
        console.error('WebSocket error:', error);
      };

      wsRef.current = ws;
    } catch (error) {
      console.error('Failed to create WebSocket:', error);
      reconnectTimeoutRef.current = window.setTimeout(connect, 3000);
    }
  }, [url]);

  const disconnect = useCallback(() => {
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = undefined;
    }
    if (wsRef.current) {
      wsRef.current.onclose = null;
      wsRef.current.close();
      wsRef.current = null;
    }
  }, []);

  const send = useCallback((type: string, data: unknown) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify({ type, data }));
      return true;
    }
    console.warn('WebSocket not ready, message not sent:', type);
    return false;
  }, []);

  useEffect(() => {
    connect();
    return () => {
      disconnect();
    };
  }, [connect, disconnect]);

  return { send, disconnect, reconnect: connect };
}
