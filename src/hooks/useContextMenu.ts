import { useState, useEffect, useRef } from "react";

interface MenuState {
  id: number;
  x: number;
  y: number;
}

export function useContextMenu() {
  const [menuState, setMenuState] = useState<MenuState | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuState(null);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  const handleContext = (e: React.MouseEvent, id: number) => {
    e.preventDefault();
    setMenuState({ id, x: e.clientX, y: e.clientY });
  };

  const closeMenu = () => setMenuState(null);

  return { menuState, menuRef, handleContext, closeMenu };
}
