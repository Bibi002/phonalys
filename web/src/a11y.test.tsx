import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { Field, Tabs } from "./components";

function Demo() {
  const [v, setV] = useState<"a" | "b" | "c">("a");
  return (
    <Tabs
      label="Démo"
      value={v}
      onChange={setV}
      tabs={[
        ["a", "Un"],
        ["b", "Deux"],
        ["c", "Trois"],
      ]}
    />
  );
}

describe("accessibilité", () => {
  it("les onglets se parcourent aux flèches, un seul est atteignable par Tab", () => {
    render(<Demo />);
    const [un, deux, trois] = screen.getAllByRole("tab");
    expect(un?.tabIndex).toBe(0);
    expect(deux?.tabIndex).toBe(-1);
    un?.focus();
    fireEvent.keyDown(un!, { key: "ArrowRight" });
    expect(screen.getByRole("tab", { name: "Deux" }).getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement).toBe(deux);
    fireEvent.keyDown(deux!, { key: "End" });
    expect(document.activeElement).toBe(trois);
    fireEvent.keyDown(trois!, { key: "ArrowRight" });
    expect(screen.getByRole("tab", { name: "Un" }).getAttribute("aria-selected")).toBe("true");
  });

  it("l'aide d'un champ est reliée au champ", () => {
    render(<Field label="Nom" value="" onChange={() => undefined} hint="Tel qu'il figure sur la carte" />);
    const input = screen.getByRole("textbox", { name: "Nom" });
    const id = input.getAttribute("aria-describedby");
    expect(id && document.getElementById(id)?.textContent).toBe("Tel qu'il figure sur la carte");
  });
});
