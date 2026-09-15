// Toast notification system (from console core)
import { writable } from 'svelte/store';

export interface Message {
  id: number;
  type: 'success' | 'error' | 'warning' | 'info';
  text: string;
}

let nextId = 0;

function createMessages() {
  const { subscribe, update } = writable<Message[]>([]);

  function add(type: Message['type'], text: string, duration = 4000) {
    const id = nextId++;
    update(msgs => [...msgs, { id, type, text }]);
    if (duration > 0) {
      setTimeout(() => remove(id), duration);
    }
    return id;
  }

  function remove(id: number) {
    update(msgs => msgs.filter(m => m.id !== id));
  }

  return {
    subscribe,
    success: (text: string) => add('success', text),
    error: (text: string) => add('error', text, 6000),
    warning: (text: string) => add('warning', text),
    info: (text: string) => add('info', text),
    remove,
  };
}

export const messages = createMessages();
