import { useMemo } from "react";
import { api, records, type RecordKind } from "./api";
import { useLoad } from "./components";
import { fullName, type Patient } from "./model";

export function usePatients() {
  const state = useLoad(async () => {
    const list = await api<Patient[]>("GET", "/api/patients");
    return list.sort((a, b) => fullName(a).localeCompare(fullName(b), "fr"));
  });
  const byId = useMemo(() => new Map((state.data ?? []).map((p) => [p.id, p])), [state.data]);
  return { ...state, byId };
}

export function useRecords<T>(kind: RecordKind, patientId?: string) {
  return useLoad(() => records.list<T>(kind, patientId), [kind, patientId]);
}
