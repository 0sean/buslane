class Agency < ApplicationRecord
  self.primary_key = :id

  has_many :routes, dependent: :destroy

  has_many :stops, -> { distinct }, through: :routes

  def self.by_id_or_noc(id_or_noc)
    id_or_noc.start_with?("noc:") ? find_by(noc: id_or_noc.delete_prefix("noc:")) : find_by(id: id_or_noc)
  end
end
